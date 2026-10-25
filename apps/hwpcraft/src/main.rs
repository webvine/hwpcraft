#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
fn main() -> anyhow::Result<()> {
    let mut file = None;
    let mut screenshot = None;
    let mut exit = false;
    let mut view = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--view" {
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
        } else {
            file = Some(std::path::PathBuf::from(arg));
        }
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1501.0, 916.0])
            .with_min_inner_size([980.0, 720.0]),
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
