use super::*;

fn input(app: &mut HwpApp, event: Event) {
    let ctx = app.ctx.clone();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 800.0))),
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
    assert!(app.error.is_none(), "{:?}", app.error);
}

fn selected_shape(app: &mut HwpApp, start: usize, end: usize) -> serde_json::Value {
    app.session
        .select(
            Cursor {
                offset: start,
                ..Default::default()
            },
            false,
        )
        .unwrap();
    app.session
        .select(
            Cursor {
                offset: end,
                ..Default::default()
            },
            true,
        )
        .unwrap();
    app.session.character_properties().unwrap()
}

#[test]
fn toolbar_pending_shape_survives_ime_preedit_and_styles_only_committed_text() {
    let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
    app.command(Command::Insert("앞뒤".into()));
    app.session
        .select(
            Cursor {
                offset: 1,
                ..Default::default()
            },
            false,
        )
        .unwrap();
    let before = app.session.character_properties().unwrap();
    let generation = app.session.generation;
    app.command(Command::Character(json!({"bold":true,"fontSize":1800})));
    assert_eq!(app.session.generation, generation);
    assert!(app.bold);
    assert_eq!(app.font_size, 18.0);
    input(
        &mut app,
        Event::Ime(ImeEvent::Preedit {
            text: "ㅎ".into(),
            active_range_chars: None,
        }),
    );
    assert_eq!(app.session.text(), "앞뒤");
    assert_eq!(app.session.generation, generation);
    input(&mut app, Event::Ime(ImeEvent::Commit("한😀".into())));
    assert_eq!(app.session.text(), "앞한😀뒤");
    app.command(Command::Undo);
    assert_eq!(app.session.text(), "앞뒤");
    assert_eq!(
        app.font_size,
        before["fontSize"].as_f64().unwrap() as f32 / 100.0
    );
    app.command(Command::Redo);
    assert_eq!(app.session.text(), "앞한😀뒤");
    let inserted = selected_shape(&mut app, 1, 3);
    assert_eq!(inserted["fontSize"], 1800);
    assert_eq!(inserted["bold"], true);
    assert_eq!(
        selected_shape(&mut app, 0, 1)["fontSize"],
        before["fontSize"]
    );
    assert_eq!(selected_shape(&mut app, 3, 4)["bold"], before["bold"]);
}

#[test]
fn paragraph_editor_diff_preserves_the_pending_style_and_unaffected_runs() {
    let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
    app.command(Command::Insert("앞뒤".into()));
    app.session
        .select(
            Cursor {
                offset: 1,
                ..Default::default()
            },
            false,
        )
        .unwrap();
    let before = app.session.character_properties().unwrap();
    app.command(Command::Character(json!({"italic":true,"fontSize":1600})));
    app.session.replace_paragraph_text("앞한😀뒤").unwrap();
    app.sync_formatting();
    assert_eq!(app.session.text(), "앞한😀뒤");
    assert!(app.italic);
    let inserted = selected_shape(&mut app, 1, 3);
    assert_eq!(inserted["italic"], true);
    assert_eq!(inserted["fontSize"], 1600);
    assert_eq!(selected_shape(&mut app, 0, 1)["italic"], before["italic"]);
    assert_eq!(
        selected_shape(&mut app, 3, 4)["fontSize"],
        before["fontSize"]
    );
}
