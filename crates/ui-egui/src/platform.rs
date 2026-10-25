//! Owner: format area (cross-platform). Operating-system integration the desktop
//! app needs on Windows 11, macOS and Linux: the window / package icon and
//! handing a PDF to the printer.
use std::path::Path;
use std::process::{Command, Stdio};

/// The app icon, drawn from `assets/icon.svg` (no bitmap files in the repository).
const ICON_SVG: &str = include_str!("../../../assets/icon.svg");

/// Window icon for Windows and Linux (title bar, taskbar, Alt+Tab). macOS takes
/// the icon from the app bundle instead. `None` only if rasterizing failed.
pub fn app_icon() -> Option<egui::IconData> {
    let size = 256;
    hwpcraft_render::rasterize_icon(ICON_SVG, size)
        .ok()
        .map(|rgba| egui::IconData {
            rgba,
            width: size,
            height: size,
        })
}

/// Write the app icon as a `size`×`size` PNG (16–1024). Release packaging uses it
/// for the macOS `.icns` and the Linux desktop entry icon.
pub fn write_icon_png(path: &Path, size: u32) -> anyhow::Result<()> {
    let rgba = hwpcraft_render::rasterize_icon(ICON_SVG, size)?;
    image::save_buffer(path, &rgba, size, size, image::ColorType::Rgba8)?;
    Ok(())
}

/// What happened to a print request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrintOutcome {
    /// Sent to the default printer.
    Printed,
    /// No direct print path; the PDF was opened in the default viewer to print there.
    Opened,
    /// Neither printing nor opening worked.
    Failed,
}

/// Print `pdf` `copies` times on the default printer, the way each OS offers it:
/// the Windows shell `Print` verb (Windows 11's default PDF app, Edge, has none,
/// so the PDF is opened instead), and CUPS `lp` on macOS and Linux, falling back
/// to `open` / `xdg-open`.
pub(crate) fn print_pdf(pdf: &Path, copies: u32) -> PrintOutcome {
    let copies = copies.clamp(1, 99);
    let printed = if cfg!(target_os = "windows") {
        (0..copies).all(|_| {
            succeeds(
                Command::new("powershell")
                    .args([
                        "-NoProfile",
                        "-NonInteractive",
                        "-WindowStyle",
                        "Hidden",
                        "-Command",
                        "Start-Process -FilePath $args[0] -Verb Print -ErrorAction Stop",
                    ])
                    .arg(pdf),
            )
        })
    } else {
        succeeds(
            Command::new("lp")
                .arg("-n")
                .arg(copies.to_string())
                .arg(pdf),
        )
    };
    if printed {
        return PrintOutcome::Printed;
    }
    if open_with_default_app(pdf) {
        PrintOutcome::Opened
    } else {
        PrintOutcome::Failed
    }
}

/// Open `path` with the OS default application.
pub(crate) fn open_with_default_app(path: &Path) -> bool {
    let mut command = if cfg!(target_os = "windows") {
        let mut c = Command::new("explorer");
        c.arg(path);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg(path);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };
    if cfg!(target_os = "windows") {
        // explorer.exe returns 1 even when it opened the file; launching is success.
        return command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok();
    }
    succeeds(&mut command)
}

fn succeeds(command: &mut Command) -> bool {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn window_icon_is_square_rgba() {
        let icon = app_icon().unwrap();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    }

    #[test]
    fn package_icon_png_round_trips() {
        let dir = std::env::temp_dir().join(format!("hwpcraft-icon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("icon-64.png");
        write_icon_png(&path, 64).unwrap();
        let png = image::open(&path).unwrap();
        assert_eq!((png.width(), png.height()), (64, 64));
        assert!(write_icon_png(&dir.join("bad.png"), 2048).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
