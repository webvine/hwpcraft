#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
fn main() -> anyhow::Result<()> {
    let mut file = None;
    let mut screenshot = None;
    let mut exit = false;
    let mut view = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--version" {
            println!("HwpCraft {}", include_str!("../../../VERSION").trim());
            return Ok(());
        } else if arg == "--view" {
            view = Some(
                args.next()
                    .ok_or_else(|| anyhow::anyhow!("--view 값이 필요합니다."))?
                    .to_string_lossy()
                    .into_owned(),
            );
        } else if arg == "--screenshot" {
            screenshot = args.next().map(std::path::PathBuf::from);
        } else if arg == "--exit-after-screenshot" {
            exit = true;
        } else if arg == "--write-icon" {
            // Packaging helper: `--write-icon <png> <size>` writes the app icon and exits.
            let path = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("--write-icon <png> <size>"))?;
            let size = args
                .next()
                .and_then(|s| s.to_str().and_then(|s| s.parse().ok()))
                .ok_or_else(|| anyhow::anyhow!("--write-icon <png> <size>"))?;
            return hwpcraft_ui_egui::write_icon_png(std::path::Path::new(&path), size);
        } else if arg.to_string_lossy().starts_with("-psn_") {
            // Older macOS Finder launches pass a process serial number; not a file.
        } else {
            file = Some(std::path::PathBuf::from(arg));
        }
    }
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1501.0, 916.0])
        .with_min_inner_size([980.0, 720.0])
        // Wayland/X11 app id: matches `hwpcraft.desktop` so Linux docks show the icon.
        .with_app_id("hwpcraft");
    // Windows and Linux title bar / taskbar icon; macOS uses the app bundle's icon.
    if let Some(icon) = hwpcraft_ui_egui::app_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "HwpCraft",
        options,
        Box::new(move |cc| {
            let mut app = hwpcraft_ui_egui::HwpApp::new(cc, file.as_deref(), screenshot, exit)?;
            if let Some(view) = view {
                app.set_initial_view(&view)?;
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
