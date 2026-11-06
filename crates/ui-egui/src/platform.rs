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

/// Each monitor's work area (the desktop minus task bars) in desktop coordinates, on
/// Windows through PowerShell's `Screen.AllScreens`; empty elsewhere or when it fails.
/// Blocks for the moment PowerShell takes to start.
pub(crate) fn monitor_work_areas() -> Vec<egui::Rect> {
    if !cfg!(target_os = "windows") {
        return Vec::new();
    }
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Add-Type -AssemblyName System.Windows.Forms; \
             [System.Windows.Forms.Screen]::AllScreens | ForEach-Object { $w = $_.WorkingArea; \
             \"$($w.X) $($w.Y) $($w.Width) $($w.Height)\" }",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    out.ok()
        .filter(|o| o.status.success())
        .map(|o| parse_work_areas(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// `x y width height` lines.
fn parse_work_areas(text: &str) -> Vec<egui::Rect> {
    text.lines()
        .filter_map(|line| {
            let n: Vec<f32> = line
                .split_whitespace()
                .map(|v| v.parse().ok())
                .collect::<Option<_>>()?;
            let [x, y, w, h] = n[..] else {
                return None;
            };
            (w >= 200.0 && h >= 150.0)
                .then(|| egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h)))
        })
        .collect()
}

/// A printer the OS knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Printer {
    pub name: String,
    pub default: bool,
}

/// The OS printers, the default one first; empty when none could be listed. Windows asks
/// WMI through PowerShell, macOS and Linux CUPS `lpstat`. Takes a moment (run it off the
/// UI thread); nothing is printed.
pub(crate) fn printers() -> Vec<Printer> {
    let text = |command: &mut Command| {
        command
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    let mut list = if cfg!(target_os = "windows") {
        parse_windows_printers(&text(Command::new("powershell").args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
             Get-CimInstance Win32_Printer | ForEach-Object { \"$($_.Default)`t$($_.Name)\" }",
        ])))
    } else {
        parse_lpstat(
            &text(Command::new("lpstat").arg("-d")),
            &text(Command::new("lpstat").arg("-a")),
        )
    };
    list.sort_by_key(|p| !p.default);
    list
}

/// `True<TAB>name` lines (PowerShell over `Win32_Printer`).
fn parse_windows_printers(text: &str) -> Vec<Printer> {
    text.lines()
        .filter_map(|line| {
            let (default, name) = line.trim_end_matches('\r').split_once('\t')?;
            let name = name.trim();
            (!name.is_empty()).then(|| Printer {
                name: name.to_owned(),
                default: default.trim().eq_ignore_ascii_case("true"),
            })
        })
        .collect()
}

/// `lpstat -d` ("system default destination: name") and `lpstat -a` ("name accepting
/// requests since …" per printer).
fn parse_lpstat(default: &str, all: &str) -> Vec<Printer> {
    let default = default
        .lines()
        .find_map(|l| l.split_once(':').map(|(_, name)| name.trim().to_owned()))
        .unwrap_or_default();
    all.lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(|name| Printer {
            name: name.to_owned(),
            default: name == default,
        })
        .collect()
}

/// Print `pdf` `copies` times on `printer` (the default one when `None`), the way each OS
/// offers it: the Windows shell `Print` / `PrintTo` verbs (Windows 11's default PDF app,
/// Edge, has neither, so the PDF is opened instead; the desktop prints through GDI
/// before it gets here), and CUPS `lp` (`-d` for a printer, `-o collate` with copies)
/// on macOS and Linux, falling back to `open` / `xdg-open`.
pub(crate) fn print_pdf(
    pdf: &Path,
    copies: u32,
    collate: bool,
    printer: Option<&str>,
) -> PrintOutcome {
    let copies = copies.clamp(1, 99);
    let printed = if cfg!(target_os = "windows") {
        (0..copies).all(|_| succeeds(&mut windows_print_command(pdf, printer)))
    } else {
        let mut lp = Command::new("lp");
        if let Some(printer) = printer {
            lp.arg("-d").arg(printer);
        }
        if copies > 1 {
            lp.arg("-o").arg(format!("collate={collate}"));
        }
        succeeds(lp.arg("-n").arg(copies.to_string()).arg(pdf))
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

/// The PowerShell command that hands `pdf` to the shell's `Print` verb, or to `PrintTo`
/// with the printer's name.
fn windows_print_command(pdf: &Path, printer: Option<&str>) -> Command {
    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
    ]);
    match printer {
        Some(printer) => {
            command
                .arg(
                    "Start-Process -FilePath $args[0] -Verb PrintTo \
                     -ArgumentList ('\"' + $args[1] + '\"') -ErrorAction Stop",
                )
                .arg(pdf)
                .arg(printer);
        }
        None => {
            command
                .arg("Start-Process -FilePath $args[0] -Verb Print -ErrorAction Stop")
                .arg(pdf);
        }
    }
    command
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
    fn work_areas_are_parsed() {
        let areas =
            parse_work_areas("0 0 5120 1392\r\n-1920 120 1920 1040\r\nbad\r\n0 0 10 10\r\n");
        assert_eq!(
            areas,
            [
                egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(5120.0, 1392.0)),
                egui::Rect::from_min_size(egui::pos2(-1920.0, 120.0), egui::vec2(1920.0, 1040.0)),
            ]
        );
    }

    /// The printer lists are read, never printed to: nothing here sends a job.
    #[test]
    fn printer_lists_are_parsed_with_the_default_first() {
        let windows = parse_windows_printers(
            "False\tMicrosoft Print to PDF\r\nTrue\t사무실 복합기\r\n\nFalse\t \r\n",
        );
        assert_eq!(
            windows,
            [
                Printer {
                    name: "Microsoft Print to PDF".into(),
                    default: false
                },
                Printer {
                    name: "사무실 복합기".into(),
                    default: true
                },
            ]
        );
        let cups = parse_lpstat(
            "system default destination: Office\n",
            "Office accepting requests since Fri 31 Oct\nPDF accepting requests since Fri 31 Oct\n",
        );
        assert_eq!(cups.iter().filter(|p| p.default).count(), 1);
        assert_eq!(cups[0].name, "Office");
        assert!(parse_lpstat("no system default destination\n", "").is_empty());
    }

    /// A chosen printer goes to `PrintTo` by name; no printer keeps the `Print` verb.
    #[test]
    fn the_windows_print_command_names_the_printer() {
        let pdf = Path::new("a.pdf");
        let args = |c: &Command| -> Vec<String> {
            c.get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        };
        let to = args(&windows_print_command(pdf, Some("사무실 복합기")));
        assert!(to.iter().any(|a| a.contains("-Verb PrintTo")), "{to:?}");
        assert_eq!(to.last().map(String::as_str), Some("사무실 복합기"));
        let default = args(&windows_print_command(pdf, None));
        assert!(
            default.iter().any(|a| a.contains("-Verb Print ")),
            "{default:?}"
        );
        assert_eq!(default.last().map(String::as_str), Some("a.pdf"));
    }

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
