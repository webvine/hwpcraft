//! Mouse drag selection on the page canvas.
use super::*;

fn frame(a: &mut HwpApp, events: Vec<Event>, modifiers: egui::Modifiers) {
    let ctx = a.ctx.clone();
    let time = ctx.input(|i| i.time) + 1.0 / 60.0;
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 1400.0))),
            time: Some(time),
            events: [vec![Event::ModifiersChanged(modifiers)], events].concat(),
            ..Default::default()
        },
        |ui| {
            let rect =
                Rect::from_min_size(Pos2::new(50.0, 50.0), Vec2::new(794.0, 1123.0) * a.zoom);
            let caret = a.session.caret().ok();
            a.page_canvas(ui, &ctx, 0, rect, caret.as_ref(), false);
        },
    );
    out.textures_delta.clear();
}

fn button(pos: Pos2, pressed: bool, modifiers: egui::Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    }
}

/// A drag from `from` to `to` in small steps, as a hand moves the mouse.
fn drag(a: &mut HwpApp, from: Pos2, to: Pos2, modifiers: egui::Modifiers) {
    frame(a, vec![Event::PointerMoved(from)], modifiers);
    frame(a, vec![button(from, true, modifiers)], modifiers);
    for i in 1..=10 {
        let p = from + (to - from) * (i as f32 / 10.0);
        frame(a, vec![Event::PointerMoved(p)], modifiers);
    }
    frame(a, vec![button(to, false, modifiers)], modifiers);
    assert!(a.error.is_none(), "{:?}", a.error);
}

/// The screen point just right of the caret at `offset` in paragraph 0.
fn point_at(a: &mut HwpApp, offset: usize) -> Pos2 {
    a.session
        .select(
            Cursor {
                offset,
                ..Default::default()
            },
            false,
        )
        .unwrap();
    let c = a.session.caret().unwrap();
    let n = |k: &str| c[k].as_f64().unwrap() as f32;
    Pos2::new(50.0, 50.0) + Vec2::new(n("x") + 1.0, n("y") + n("height") / 2.0) * a.zoom
}

fn app_with_text() -> HwpApp {
    let mut a = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
    a.session
        .run(
            "text.insert",
            json!({"text": "abcdefghijklmnopqrstuvwxyz0123456789"}),
        )
        .unwrap();
    a
}

fn select(a: &mut HwpApp, start: usize, end: usize) {
    for (offset, extend) in [(start, false), (end, true)] {
        a.session
            .select(
                Cursor {
                    offset,
                    ..Default::default()
                },
                extend,
            )
            .unwrap();
    }
}

/// A new drag selects from where the mouse was pressed, whether a selection was
/// already there or only the caret stood elsewhere.
#[test]
fn a_drag_selects_from_the_press_point_over_an_earlier_selection() {
    let mut a = app_with_text();
    let (from, to) = (point_at(&mut a, 20), point_at(&mut a, 30));

    // An earlier selection elsewhere.
    select(&mut a, 2, 6);
    drag(&mut a, from, to, egui::Modifiers::NONE);
    assert_eq!(
        a.session.selection(),
        20..30,
        "{:?}",
        a.session.selected_text()
    );

    // Only the caret elsewhere.
    select(&mut a, 3, 3);
    drag(&mut a, from, to, egui::Modifiers::NONE);
    assert_eq!(a.session.selection(), 20..30);

    // A drag that starts inside the current selection starts a new one there too.
    select(&mut a, 15, 34);
    drag(&mut a, from, to, egui::Modifiers::NONE);
    assert_eq!(a.session.selection(), 20..30);

    // Backwards: the press point is the anchor.
    select(&mut a, 2, 6);
    drag(&mut a, to, from, egui::Modifiers::NONE);
    assert_eq!(a.session.selection(), 20..30);
    assert_eq!(a.session.cursor.offset, 20);
}

/// Shift+drag still extends the current selection from its anchor.
#[test]
fn a_shift_drag_extends_the_current_selection() {
    let mut a = app_with_text();
    let (from, to) = (point_at(&mut a, 20), point_at(&mut a, 30));
    select(&mut a, 2, 6);
    drag(&mut a, from, to, egui::Modifiers::SHIFT);
    assert_eq!(a.session.selection(), 2..30);
}
