//! Visual editing in the 수식 편집기 preview: click a drawn piece to put the
//! script caret there, type into the preview, Tab between empty `{}` slots.
//!
//! The engine's `equation_layout` ties each drawn piece to the script text that
//! made it; pieces it could not tie keep no range and never move the caret to a
//! guessed place. Offsets count Unicode scalars, like the script editor.
use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use hwpcraft_engine::{EquationLayout, EquationLayoutItem};

/// Script range of a piece that was tied to the script.
fn range(item: &EquationLayoutItem) -> Option<(usize, usize)> {
    item.start.zip(item.end)
}

fn area(item: &EquationLayoutItem) -> f64 {
    item.width.max(0.5) * item.height.max(0.5)
}

fn contains(item: &EquationLayoutItem, x: f64, y: f64) -> bool {
    x >= item.x && x <= item.x + item.width && y >= item.y && y <= item.y + item.height
}

fn distance(item: &EquationLayoutItem, x: f64, y: f64) -> f64 {
    let dx = (item.x - x).max(x - (item.x + item.width)).max(0.0);
    let dy = (item.y - y).max(y - (item.y + item.height)).max(0.0);
    dx.hypot(dy)
}

/// The caret a click at (`x`, `y`, layout units) asks for: the smallest tied
/// piece under the pointer, else the nearest tied piece; its start when the
/// click is on the left half, its end on the right. `None` when nothing is tied.
pub(crate) fn hit(layout: &EquationLayout, x: f64, y: f64) -> Option<usize> {
    let tied = || layout.items.iter().filter(|i| range(i).is_some());
    let item = tied()
        .filter(|i| contains(i, x, y))
        .min_by(|a, b| area(a).total_cmp(&area(b)))
        .or_else(|| tied().min_by(|a, b| distance(a, x, y).total_cmp(&distance(b, x, y))))?;
    let (start, end) = range(item)?;
    Some(if x < item.x + item.width / 2.0 {
        start
    } else {
        end
    })
}

/// The next (or previous) empty slot from `caret`, wrapping around.
pub(crate) fn next_slot(slots: &[usize], caret: usize, backward: bool) -> Option<usize> {
    if backward {
        slots
            .iter()
            .rev()
            .find(|&&s| s < caret)
            .or_else(|| slots.last())
            .copied()
    } else {
        slots
            .iter()
            .find(|&&s| s > caret)
            .or_else(|| slots.first())
            .copied()
    }
}

/// Where the caret line goes, in layout units: (x, top, height).
pub(crate) fn caret_line(layout: &EquationLayout, caret: usize) -> Option<(f64, f64, f64)> {
    let items = || layout.items.iter().filter(|i| range(i).is_some());
    if let Some(slot) = items().find(|i| i.kind == "slot" && i.start == Some(caret)) {
        return Some((slot.x + slot.width / 2.0, slot.y, slot.height));
    }
    // Inside a piece: proportional to the characters before the caret.
    let inside = items()
        .filter(|i| range(i).is_some_and(|(s, e)| s <= caret && caret <= e && s < e))
        .min_by(|a, b| area(a).total_cmp(&area(b)));
    if let Some(i) = inside {
        let (s, e) = range(i)?;
        let t = (caret - s) as f64 / (e - s) as f64;
        return Some((i.x + i.width * t, i.y, i.height));
    }
    // Between pieces: after the last piece that ends before the caret.
    items()
        .filter(|i| i.end.is_some_and(|e| e <= caret))
        .max_by_key(|i| i.end)
        .map(|i| (i.x + i.width, i.y, i.height))
        .or(Some((0.0, 0.0, layout.height)))
}

/// The tied piece the caret is in or touches, for highlighting.
pub(crate) fn current(layout: &EquationLayout, caret: usize) -> Option<&EquationLayoutItem> {
    layout
        .items
        .iter()
        .filter(|i| i.kind != "slot")
        .filter(|i| range(i).is_some_and(|(s, e)| s <= caret && caret <= e))
        .min_by(|a, b| area(a).total_cmp(&area(b)))
}

/// Caret just inside the first empty `{}` of an inserted template, if any.
pub(crate) fn first_slot(snippet: &str) -> Option<usize> {
    let chars: Vec<char> = snippet.chars().collect();
    chars
        .windows(2)
        .position(|w| w == ['{', '}'])
        .map(|i| i + 1)
}

fn byte_at(script: &str, caret: usize) -> usize {
    script
        .char_indices()
        .nth(caret)
        .map_or(script.len(), |(i, _)| i)
}

/// Apply one key or text event typed into the preview. Returns true when the
/// event was used.
pub(crate) fn apply_event(
    script: &mut String,
    caret: &mut usize,
    slots: &[usize],
    event: &egui::Event,
) -> bool {
    let count = script.chars().count();
    *caret = (*caret).min(count);
    match event {
        egui::Event::Text(text) | egui::Event::Ime(egui::ImeEvent::Commit(text)) => {
            script.insert_str(byte_at(script, *caret), text);
            *caret += text.chars().count();
        }
        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => match key {
            egui::Key::Backspace if *caret > 0 => {
                let (a, b) = (byte_at(script, *caret - 1), byte_at(script, *caret));
                script.replace_range(a..b, "");
                *caret -= 1;
            }
            egui::Key::Delete if *caret < count => {
                let (a, b) = (byte_at(script, *caret), byte_at(script, *caret + 1));
                script.replace_range(a..b, "");
            }
            egui::Key::ArrowLeft => *caret = caret.saturating_sub(1),
            egui::Key::ArrowRight => *caret = (*caret + 1).min(count),
            egui::Key::Home => *caret = 0,
            egui::Key::End => *caret = count,
            egui::Key::Tab => {
                if let Some(s) = next_slot(slots, *caret, modifiers.shift) {
                    *caret = s;
                }
            }
            _ => return false,
        },
        _ => return false,
    }
    true
}

/// Draw the slot boxes, the current piece and the caret over the preview image
/// placed at `dest`.
pub(crate) fn paint(
    painter: &egui::Painter,
    dest: Rect,
    layout: &EquationLayout,
    caret: usize,
    focused: bool,
) {
    if layout.width <= 0.0 || layout.height <= 0.0 {
        return;
    }
    let (sx, sy) = (
        dest.width() / layout.width as f32,
        dest.height() / layout.height as f32,
    );
    let rect = |x: f64, y: f64, w: f64, h: f64| {
        Rect::from_min_size(
            dest.min + Vec2::new(x as f32 * sx, y as f32 * sy),
            Vec2::new(w as f32 * sx, h as f32 * sy),
        )
    };
    let accent = crate::chrome::Tokens::ACCENT;
    for slot in layout.items.iter().filter(|i| i.kind == "slot") {
        let r = rect(slot.x, slot.y, slot.width, slot.height).shrink(1.0);
        let stroke = Stroke::new(1.0, Color32::from_gray(140));
        for (a, b) in [
            (r.left_top(), r.right_top()),
            (r.right_top(), r.right_bottom()),
            (r.right_bottom(), r.left_bottom()),
            (r.left_bottom(), r.left_top()),
        ] {
            painter.extend(egui::Shape::dashed_line(&[a, b], stroke, 3.0, 2.0));
        }
    }
    if let Some(i) = current(layout, caret) {
        painter.rect_filled(
            rect(i.x, i.y, i.width, i.height),
            1.0,
            accent.gamma_multiply(0.12),
        );
    }
    if focused && let Some((x, y, h)) = caret_line(layout, caret) {
        let top = dest.min + Vec2::new(x as f32 * sx, y as f32 * sy);
        painter.line_segment(
            [top, Pos2::new(top.x, top.y + h as f32 * sy)],
            Stroke::new(1.5, accent),
        );
    }
}

/// The 수식 매크로 menu: saved scripts to insert at the caret, and saving the
/// current script under a name. Returns a script to insert.
pub(crate) fn macro_menu(
    ui: &mut egui::Ui,
    script: &str,
    name: &mut String,
    note: &mut Option<String>,
) -> Option<String> {
    let mut chosen = None;
    ui.menu_button(egui::RichText::new("매크로").size(13.0), |ui| {
        ui.set_min_width(260.0);
        ui.label(egui::RichText::new("수식 매크로").strong());
        ui.separator();
        match hwpcraft_engine::equation_macros() {
            Ok(list) if list.is_empty() => {
                ui.label(egui::RichText::new("저장한 수식 매크로가 없습니다.").weak());
            }
            Ok(list) => {
                for m in list {
                    ui.horizontal(|ui| {
                        if ui.button(&m.name).on_hover_text(&m.script).clicked() {
                            chosen = Some(format!("{} ", m.script));
                            ui.close();
                        }
                        if ui.small_button("×").on_hover_text("지우기").clicked() {
                            *note = hwpcraft_engine::delete_equation_macro(&m.name)
                                .err()
                                .map(|e| format!("{e:#}"));
                        }
                    });
                }
            }
            Err(e) => {
                ui.colored_label(Color32::DARK_RED, format!("{e:#}"));
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(name)
                    .hint_text("이름")
                    .desired_width(150.0),
            );
            if ui
                .add_enabled(!name.trim().is_empty(), egui::Button::new("지금 수식 저장"))
                .clicked()
            {
                *note = match hwpcraft_engine::save_equation_macro(name, script) {
                    Ok(()) => {
                        name.clear();
                        None
                    }
                    Err(e) => Some(format!("{e:#}")),
                };
            }
        });
        if let Some(n) = note.as_deref() {
            ui.colored_label(Color32::DARK_RED, n);
        }
    })
    .response
    .on_hover_text("수식 매크로: 자주 쓰는 수식을 이름으로 저장하고 넣습니다");
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(
        text: &str,
        x: f64,
        w: f64,
        start: Option<usize>,
        end: Option<usize>,
    ) -> EquationLayoutItem {
        EquationLayoutItem {
            kind: if text.is_empty() { "slot" } else { "text" },
            text: text.into(),
            x,
            y: 0.0,
            width: w,
            height: 10.0,
            start,
            end,
        }
    }

    fn layout(items: Vec<EquationLayoutItem>, slots: Vec<usize>) -> EquationLayout {
        EquationLayout {
            width: 100.0,
            height: 10.0,
            baseline: 8.0,
            items,
            slots,
        }
    }

    #[test]
    fn clicks_land_on_tied_pieces_only() {
        let l = layout(
            vec![
                item("ab", 0.0, 20.0, Some(0), Some(2)),
                item("?", 30.0, 10.0, None, None),
                item("c", 50.0, 10.0, Some(9), Some(10)),
            ],
            vec![],
        );
        assert_eq!(hit(&l, 2.0, 5.0), Some(0));
        assert_eq!(hit(&l, 18.0, 5.0), Some(2));
        // An untied piece goes to the nearest tied one, never to a guess.
        assert_eq!(hit(&l, 32.0, 5.0), Some(2));
        assert_eq!(hit(&l, 38.0, 5.0), Some(9));
        assert_eq!(hit(&l, 58.0, 5.0), Some(10));
        assert_eq!(
            hit(
                &layout(vec![item("?", 0.0, 9.0, None, None)], vec![]),
                1.0,
                1.0
            ),
            None
        );
    }

    #[test]
    fn tab_walks_slots_and_typing_edits_at_the_caret() {
        assert_eq!(next_slot(&[1, 9], 0, false), Some(1));
        assert_eq!(next_slot(&[1, 9], 1, false), Some(9));
        assert_eq!(next_slot(&[1, 9], 9, false), Some(1));
        assert_eq!(next_slot(&[1, 9], 9, true), Some(1));
        assert_eq!(next_slot(&[1, 9], 1, true), Some(9));
        assert_eq!(next_slot(&[], 3, false), None);
        assert_eq!(first_slot("{} over {} "), Some(1));
        assert_eq!(first_slot("alpha "), None);

        let (mut script, mut caret) = ("{} over {}".to_owned(), 1);
        assert!(apply_event(
            &mut script,
            &mut caret,
            &[1, 9],
            &egui::Event::Text("가".into())
        ));
        assert_eq!((script.as_str(), caret), ("{가} over {}", 2));
        let tab = egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert!(apply_event(&mut script, &mut caret, &[10], &tab));
        assert_eq!(caret, 10);
        let back = egui::Event::Key {
            key: egui::Key::Backspace,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert!(apply_event(&mut script, &mut caret, &[], &back));
        assert_eq!((script.as_str(), caret), ("{가} over }", 9));
    }

    #[test]
    fn caret_sits_in_slots_and_between_pieces() {
        let l = layout(
            vec![
                item("ab", 0.0, 20.0, Some(0), Some(2)),
                item("", 40.0, 6.0, Some(6), Some(6)),
            ],
            vec![6],
        );
        assert_eq!(caret_line(&l, 6), Some((43.0, 0.0, 10.0)));
        assert_eq!(caret_line(&l, 1), Some((10.0, 0.0, 10.0)));
        assert_eq!(caret_line(&l, 4), Some((20.0, 0.0, 10.0)));
        assert_eq!(current(&l, 1).map(|i| i.text.as_str()), Some("ab"));
    }
    /// The real editor: click the denominator slot in the preview, then type.
    #[test]
    fn clicking_a_preview_slot_and_typing_fills_it() {
        let mut app =
            crate::HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        app.open_equation_editor(None);
        if let Some(d) = app.ext.object.equation.as_mut() {
            d.script = "{} over {}".into();
            d.caret = 0;
        }
        let ctx = app.ctx.clone();
        let frame = |app: &mut crate::HwpApp, events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 900.0))),
                    events,
                    ..Default::default()
                },
                |ui| app.equation_editor_dialog(ui.ctx()),
            );
            output.textures_delta.clear();
        };
        frame(&mut app, vec![]);
        frame(&mut app, vec![]);
        let rect = ctx
            .read_response(egui::Id::new("equation-preview"))
            .map(|r| r.rect)
            .unwrap();
        let (_, _, size) = app.equation_preview.clone().unwrap();
        let scale = (rect.width() / size.x).min(rect.height() / size.y).min(1.0);
        let dest = Rect::from_center_size(rect.center(), size * scale);
        let layout = app
            .ext
            .object
            .equation
            .as_ref()
            .unwrap()
            .layout
            .clone()
            .unwrap()
            .1;
        let denominator = layout
            .items
            .iter()
            .find(|i| i.kind == "slot" && i.start == Some(9))
            .unwrap();
        let at = dest.min
            + Vec2::new(
                ((denominator.x + denominator.width / 2.0) / layout.width) as f32 * dest.width(),
                ((denominator.y + denominator.height / 2.0) / layout.height) as f32 * dest.height(),
            );
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut app, vec![egui::Event::PointerMoved(at), button(true)]);
        frame(&mut app, vec![button(false)]);
        frame(&mut app, vec![egui::Event::Text("y".into())]);
        let d = app.ext.object.equation.as_ref().unwrap();
        assert_eq!(d.script, "{} over {y}");
        assert_eq!(d.caret, 10);
        // Tab wraps to the numerator slot; typing fills it.
        let tab = egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut app, vec![tab]);
        frame(&mut app, vec![egui::Event::Text("x".into())]);
        assert_eq!(
            app.ext.object.equation.as_ref().unwrap().script,
            "{x} over {y}"
        );
    }
}
