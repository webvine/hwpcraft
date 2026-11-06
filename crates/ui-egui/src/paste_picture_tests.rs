//! Ctrl+V and 붙이기 put a copied picture in at the caret.
use super::*;

/// A clipboard holding `text` and, when `picture`, a 96 × 48 bitmap.
pub(crate) fn fake_clipboard(text: Option<&str>, picture: bool) -> anyhow::Result<()> {
    let png = if picture {
        let img = image::RgbImage::from_fn(96, 48, |x, y| image::Rgb([x as u8, y as u8, 90]));
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
        Some(png)
    } else {
        None
    };
    clipboard::FAKE.with(|fake| {
        *fake.borrow_mut() = Some(clipboard::Fake {
            text: text.map(str::to_owned),
            png,
        })
    });
    Ok(())
}

pub(crate) fn pictures(app: &HwpApp) -> usize {
    app.session
        .objects()
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["kind"] == "picture")
        .count()
}

fn input(app: &mut HwpApp, time: f64, event: Event) {
    let ctx = app.ctx.clone();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 800.0))),
            time: Some(time),
            events: vec![event],
            ..Default::default()
        },
        |ui| {
            ui.interact(
                Rect::from_min_size(Pos2::new(20.0, 20.0), Vec2::new(700.0, 500.0)),
                egui::Id::new("document-canvas"),
                egui::Sense::click_and_drag(),
            )
            .request_focus();
            app.keyboard(ui.ctx());
        },
    );
    output.textures_delta.clear();
}

fn v_up(modifiers: egui::Modifiers) -> Event {
    Event::Key {
        key: Key::V,
        physical_key: Some(Key::V),
        pressed: false,
        repeat: false,
        modifiers,
    }
}

/// What egui-winit sends for a Ctrl+V: the paste (only when there is text; the V
/// press itself never comes), then V coming up under Ctrl.
fn ctrl_v(app: &mut HwpApp, time: f64, text: Option<&str>) {
    let ctrl = egui::Modifiers::COMMAND;
    input(app, time, Event::ModifiersChanged(ctrl));
    if let Some(text) = text {
        input(app, time, Event::Paste(text.into()));
    }
    input(app, time + 0.1, v_up(ctrl));
    input(
        app,
        time + 0.2,
        Event::ModifiersChanged(egui::Modifiers::NONE),
    );
}

fn app() -> anyhow::Result<HwpApp> {
    let mut app = HwpApp::with_context(egui::Context::default(), None, None, false)?;
    app.command(Command::Insert("앞".into()));
    Ok(app)
}

#[test]
fn ctrl_v_with_only_a_picture_on_the_clipboard_puts_it_at_the_caret() -> anyhow::Result<()> {
    let mut app = app()?;
    fake_clipboard(None, true)?;
    ctrl_v(&mut app, 1.0, None);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(pictures(&app), 1);
    // Like a character, after 앞, with the caret after it.
    let a = app.session.objects()[0]["address"].clone();
    let p = app.session.object_properties(serde_json::from_value(a)?)?["props"].clone();
    assert_eq!(p["treatAsChar"], true, "{p}");
    assert!(app.session.cursor.offset >= 1);
    // One undo takes it out again.
    app.command(Command::Undo);
    assert_eq!(pictures(&app), 0);
    assert_eq!(app.session.text(), "앞");
    Ok(())
}

#[test]
fn text_on_the_clipboard_is_pasted_as_text_once() -> anyhow::Result<()> {
    let mut app = app()?;
    // Text and a picture (as a spreadsheet copies): the text is what is pasted, and
    // the V coming up after it pastes nothing more.
    fake_clipboard(Some("글"), true)?;
    ctrl_v(&mut app, 1.0, Some("글"));
    assert_eq!(pictures(&app), 0);
    assert_eq!(app.session.text(), "앞글");
    // A picture copied right after (no text now) is pasted by the next Ctrl+V.
    fake_clipboard(None, true)?;
    ctrl_v(&mut app, 1.5, None);
    assert_eq!(pictures(&app), 1);
    // Nothing on the clipboard: Ctrl+V does nothing and says nothing.
    fake_clipboard(None, false)?;
    ctrl_v(&mut app, 5.0, None);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(pictures(&app), 1);
    Ok(())
}

#[test]
fn letting_go_of_ctrl_before_v_still_pastes_the_picture() -> anyhow::Result<()> {
    let mut app = app()?;
    fake_clipboard(None, true)?;
    let ctrl = egui::Modifiers::COMMAND;
    // Ctrl comes up first, then V.
    input(&mut app, 1.0, Event::ModifiersChanged(ctrl));
    input(
        &mut app,
        1.2,
        Event::ModifiersChanged(egui::Modifiers::NONE),
    );
    input(&mut app, 1.3, v_up(egui::Modifiers::NONE));
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(pictures(&app), 1);
    // Typing v is typing.
    input(
        &mut app,
        3.0,
        Event::Key {
            key: Key::V,
            physical_key: Some(Key::V),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    input(&mut app, 3.0, Event::Text("v".into()));
    input(&mut app, 3.1, v_up(egui::Modifiers::NONE));
    assert_eq!(pictures(&app), 1);
    assert!(app.session.text().ends_with('v'));
    Ok(())
}

#[test]
fn a_picture_copied_in_a_browser_comes_without_its_address() -> anyhow::Result<()> {
    let mut app = app()?;
    fake_clipboard(Some("https://example.com/그림.png"), true)?;
    ctrl_v(&mut app, 1.0, Some("https://example.com/그림.png"));
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(pictures(&app), 1);
    assert!(!app.session.text().contains("example"));
    // The same address without a picture is text.
    fake_clipboard(Some("https://example.com/"), false)?;
    ctrl_v(&mut app, 5.0, Some("https://example.com/"));
    assert!(app.session.text().contains("https://example.com/"));
    assert_eq!(pictures(&app), 1);
    Ok(())
}

#[test]
fn a_distribution_document_takes_no_pasted_picture() -> anyhow::Result<()> {
    let mut app = app()?;
    app.session.core.document_mut().header.distribution = true;
    fake_clipboard(None, true)?;
    ctrl_v(&mut app, 1.0, None);
    assert_eq!(pictures(&app), 0);
    Ok(())
}
