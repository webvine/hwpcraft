//! Typing on the page inside a nested table cell or a text box. The body caret cannot
//! stand there, so a click there starts the engine's nested caret and the keys below
//! edit that paragraph; Esc or a click in the body returns to the body caret. Ribbon and
//! toolbar formatting goes to the nested caret too (`nested_command`).
use crate::{HwpApp, clipboard};
use egui::{Event, ImeEvent, Key};
use hwpcraft_engine::{Command, Motion};
use serde_json::{Value, json};

impl HwpApp {
    /// Handle one canvas event while the nested caret is active; `false` leaves it to
    /// the body handlers (file and window shortcuts).
    pub(crate) fn nested_event(&mut self, ctx: &egui::Context, event: &Event) -> bool {
        let edited = match event {
            Event::Text(t) if self.preedit.is_empty() => self.session.nested_insert(t),
            Event::Ime(ImeEvent::Preedit { text, .. }) => {
                self.preedit = text.clone();
                return true;
            }
            Event::Ime(ImeEvent::Commit(text)) => {
                self.preedit.clear();
                self.session.nested_insert(text)
            }
            Event::Copy | Event::Cut => {
                if let Ok(text) = self.session.nested_selected_text()
                    && !text.is_empty()
                    && !clipboard::set(&text, None)
                {
                    ctx.copy_text(text);
                }
                if matches!(event, Event::Cut) {
                    self.session.nested_insert("")
                } else {
                    return true;
                }
            }
            Event::Paste(text) => self.session.nested_insert(text),
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command => match key {
                Key::Z => {
                    self.command(if modifiers.shift {
                        Command::Redo
                    } else {
                        Command::Undo
                    });
                    self.session.revalidate_nested();
                    return true;
                }
                Key::B | Key::I | Key::U => {
                    let name = match key {
                        Key::B => "bold",
                        Key::I => "italic",
                        _ => "underline",
                    };
                    let on = match name {
                        "bold" => &mut self.bold,
                        "italic" => &mut self.italic,
                        _ => &mut self.underline,
                    };
                    *on = !*on;
                    self.session.nested_character(json!({ (name): *on }))
                }
                Key::A => {
                    let r = self
                        .session
                        .nested_move(Motion::LineStart, false)
                        .and_then(|()| self.session.nested_move(Motion::LineEnd, true));
                    self.result(r);
                    return true;
                }
                // File, find and window shortcuts keep working.
                _ => return false,
            },
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if self.preedit.is_empty() => match key {
                Key::Enter => self.session.nested_enter(),
                // Next / previous cell of a nested table; a tab character in a text box.
                Key::Tab => match self.session.nested_next_cell(modifiers.shift) {
                    Ok(false) if !modifiers.shift => self.session.nested_insert("\t"),
                    r => r.map(|_| ()),
                },
                Key::Backspace => self.session.nested_backspace(),
                Key::Delete => self.session.nested_delete(),
                Key::ArrowLeft
                | Key::ArrowRight
                | Key::ArrowUp
                | Key::ArrowDown
                | Key::Home
                | Key::End => {
                    let motion = match key {
                        Key::ArrowLeft => Motion::Left,
                        Key::ArrowRight => Motion::Right,
                        Key::ArrowUp => Motion::Up,
                        Key::ArrowDown => Motion::Down,
                        Key::Home => Motion::LineStart,
                        _ => Motion::LineEnd,
                    };
                    self.session.nested_move(motion, modifiers.shift)
                }
                // Other keys must not reach the body caret while this one is active.
                _ => return true,
            },
            _ => return false,
        };
        self.result(edited);
        self.scroll_to_caret = true;
        true
    }

    /// Character or paragraph formatting from the ribbon, the toolbar or `format.*` while
    /// the nested caret is active: it applies to the nested paragraph (or its selection),
    /// not to the body. Undo and redo keep the nested caret where it can stay. `None`
    /// leaves the command to the body.
    pub(crate) fn nested_command(&mut self, command: &Command) -> Option<bool> {
        self.session.nested_caret()?;
        let r = match command {
            Command::Character(format) => {
                for (key, on) in [
                    ("bold", &mut self.bold),
                    ("italic", &mut self.italic),
                    ("underline", &mut self.underline),
                ] {
                    if let Some(value) = format[key].as_bool() {
                        *on = value;
                    }
                }
                self.session.nested_character(format.clone())
            }
            Command::Paragraph(format) => self.session.nested_paragraph_format(format.clone()),
            Command::Undo | Command::Redo => {
                let r = self.session.execute(if matches!(command, Command::Undo) {
                    Command::Undo
                } else {
                    Command::Redo
                });
                self.session.revalidate_nested();
                r
            }
            _ => return None,
        };
        Some(self.result(r).is_some())
    }

    /// `format.character` / `format.paragraph` run by id, for `nested_command`.
    pub(crate) fn nested_run(&mut self, id: &str, params: &Value) -> Option<bool> {
        match id {
            "format.character" => self.nested_command(&Command::Character(params.clone())),
            "format.paragraph" => self.nested_command(&Command::Paragraph(params.clone())),
            _ => None,
        }
    }

    /// Double / triple click while the nested caret is active: the word or paragraph.
    pub(crate) fn nested_multi_click(&mut self, triple: bool) {
        let r = if triple {
            self.session.nested_select_paragraph()
        } else {
            self.session.nested_select_word()
        };
        self.result(r);
    }

    /// Start (or move) the nested caret where a click the body caret refused landed.
    /// Returns `false` when the point is not in a nested table cell or text box.
    pub(crate) fn nested_click(&mut self, page: u32, x: f64, y: f64, extend: bool) -> bool {
        match self.session.hit_nested(page, x, y) {
            Ok(Some(caret)) => {
                let first = self.session.nested_caret().is_none();
                let r = self.session.set_nested(caret, extend);
                if self.result(r).is_some() && first {
                    self.message =
                        "표 안의 표·글상자를 편집합니다. 본문으로 돌아가려면 Esc를 누르세요."
                            .into();
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::HwpApp;
    use egui::{Event, Key, Pos2, Rect, Vec2};
    use hwpcraft_engine::{ContextAddress, ContextStep};
    use serde_json::json;

    /// One frame of the real page canvas and keyboard, page 0 at (50, 50).
    fn frame(a: &mut HwpApp, events: Vec<Event>) {
        frame_after(a, events, 1.0);
    }
    /// A frame `seconds` after the previous one.
    fn frame_after(a: &mut HwpApp, events: Vec<Event>, seconds: f64) {
        let ctx = a.ctx.clone();
        let time = ctx.input(|i| i.time) + seconds;
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 1400.0))),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| {
                let rect =
                    Rect::from_min_size(Pos2::new(50.0, 50.0), Vec2::new(794.0, 1123.0) * a.zoom);
                // The shared focus target the document view registers every frame.
                let canvas = egui::Id::new("document-canvas");
                ui.interact(
                    ui.clip_rect(),
                    canvas,
                    egui::Sense::focusable_noninteractive(),
                );
                // As the document view: Tab and arrows stay on the canvas.
                ctx.memory_mut(|m| {
                    m.set_focus_lock_filter(
                        canvas,
                        egui::EventFilter {
                            tab: true,
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            escape: false,
                        },
                    )
                });
                let caret = if a.session.nested_caret().is_some() {
                    a.session.nested_caret_rect().ok()
                } else {
                    a.session.caret().ok()
                };
                a.page_canvas(ui, &ctx, 0, rect, caret.as_ref(), false);
                a.keyboard(&ctx);
            },
        );
        out.textures_delta.clear();
    }
    fn click(p: Pos2) -> Vec<Event> {
        let button = |pressed| Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        vec![Event::PointerMoved(p), button(true), button(false)]
    }
    fn key(key: Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// An app with a 2×2 table nested in the first cell of a 2×2 table, the nested first
    /// cell's address and a screen point inside it.
    fn nested_table() -> anyhow::Result<(HwpApp, ContextAddress, Pos2)> {
        let ctx = egui::Context::default();
        let mut a = HwpApp::with_context(ctx, None, None, false)?;
        a.session
            .run("insert.table", json!({"rows": 2, "columns": 2}))?;
        let created = a.session.run(
            "context.table.create",
            json!({"address":{"section":0,"paragraph":0,"path":[{"control":2,"cell":0,"paragraph":0}]},
                   "rows":2,"columns":2}),
        )?;
        let steps = created["address"]["path"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let target = ContextAddress {
            version: 2,
            section: 0,
            paragraph: created["address"]["paragraph"].as_u64().unwrap_or(0) as usize,
            path: steps
                .iter()
                .map(|v| ContextStep::TableCell {
                    control: v["control"].as_u64().unwrap_or(0) as usize,
                    cell: v["cell"].as_u64().unwrap_or(0) as usize,
                    paragraph: v["paragraph"].as_u64().unwrap_or(0) as usize,
                })
                .collect(),
        };
        // A page point inside the nested cell, on screen.
        let mut point = None;
        'search: for y in (0..1100).step_by(4) {
            for x in (0..800).step_by(8) {
                if let Ok(Some(c)) = a.session.hit_nested(0, f64::from(x), f64::from(y))
                    && c.address == target
                {
                    point = Some(Pos2::new(50.0, 50.0) + Vec2::new(x as f32, y as f32) * a.zoom);
                    break 'search;
                }
            }
        }
        let point = point.ok_or_else(|| anyhow::anyhow!("no point in the nested cell"))?;
        Ok((a, target, point))
    }

    fn key_with(key: Key, modifiers: egui::Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// The paragraph a nested table-cell address names.
    fn paragraph<'a>(
        a: &'a HwpApp,
        address: &ContextAddress,
    ) -> &'a hwpcraft_doc::model::paragraph::Paragraph {
        let mut p =
            &a.session.core.document().sections[address.section].paragraphs[address.paragraph];
        for step in &address.path {
            let ContextStep::TableCell {
                control,
                cell,
                paragraph,
            } = step
            else {
                panic!("table cells only");
            };
            let hwpcraft_doc::model::control::Control::Table(t) = &p.controls[*control] else {
                panic!("not a table");
            };
            p = &t.cells[*cell].paragraphs[*paragraph];
        }
        p
    }

    /// A click in a table inside a table cell puts the caret there; typing, Backspace
    /// and Enter edit that cell, and Esc returns to the body caret.
    #[test]
    fn clicking_into_a_nested_table_cell_edits_it_on_the_page() -> anyhow::Result<()> {
        let (mut a, target, point) = nested_table()?;
        frame(&mut a, vec![]);
        frame(&mut a, click(point));
        assert!(a.error.is_none(), "{:?}", a.error);
        assert_eq!(
            a.session.nested_caret().map(|c| c.address.clone()),
            Some(target.clone())
        );

        frame(&mut a, vec![Event::Text("가나".into())]);
        frame(&mut a, vec![key(Key::Backspace)]);
        frame(
            &mut a,
            vec![
                Event::Text("다".into()),
                key(Key::Enter),
                Event::Text("라".into()),
            ],
        );
        let text = |a: &mut HwpApp, address: &ContextAddress| -> anyhow::Result<String> {
            let v = a
                .session
                .run("context.v2.get", json!({ "address": address }))?;
            Ok(v["text"].as_str().unwrap_or_default().to_owned())
        };
        assert_eq!(text(&mut a, &target)?, "가다");
        let second = a
            .session
            .nested_caret()
            .map(|c| c.address.clone())
            .expect("caret");
        assert_eq!(text(&mut a, &second)?, "라");
        // The body was not touched.
        assert_eq!(
            a.session.core.document().sections[0].paragraphs[0]
                .text
                .trim_matches(|c: char| c < ' '),
            ""
        );

        frame(&mut a, vec![key(Key::Escape)]);
        assert!(a.session.nested_caret().is_none());
        assert!(a.error.is_none(), "{:?}", a.error);
        Ok(())
    }

    /// Home/End go to the start and end of the caret's line in a nested cell, and a
    /// double / triple click selects the word / the paragraph.
    #[test]
    fn home_end_and_multi_clicks_work_on_lines_and_words_of_a_nested_cell() -> anyhow::Result<()> {
        let (mut a, target, point) = nested_table()?;
        frame(&mut a, vec![]);
        frame(&mut a, click(point));
        let words = "가나다 라마바 사아자 차카타 파하 ".repeat(6);
        frame(&mut a, vec![Event::Text(words.clone())]);
        let len = words.chars().count();
        let row = |a: &HwpApp| {
            a.session
                .nested_caret_rect()
                .map(|v| v["y"].as_f64().unwrap_or(0.0))
        };
        let last_row = row(&a)?;
        frame(&mut a, vec![key(Key::Home)]);
        let start = a.session.nested_caret().map_or(0, |c| c.offset);
        assert!(
            start > 0 && start < len,
            "the text wraps: line start {start} of {len}"
        );
        assert_eq!(row(&a)?, last_row, "Home stays on the line");
        frame(&mut a, vec![key(Key::ArrowLeft), key(Key::End)]);
        let end = a.session.nested_caret().map_or(0, |c| c.offset);
        assert!(end < start, "End of the line above: {end} < {start}");
        frame(&mut a, vec![key(Key::Home)]);
        assert!(a.session.nested_caret().map_or(0, |c| c.offset) < end);

        // Double click: the word under the caret; triple click: the paragraph.
        a.session
            .set_nested(hwpcraft_engine::NestedCaret::at(target.clone(), 1), false)?;
        a.nested_multi_click(false);
        assert_eq!(a.session.nested_selected_text()?, "가나다");
        a.nested_multi_click(true);
        assert_eq!(a.session.nested_selected_text()?, words);
        // The real canvas gesture reaches the same selection.
        a.session
            .set_nested(hwpcraft_engine::NestedCaret::at(target.clone(), 0), false)?;
        let point = {
            let v = a.session.nested_caret_rect()?;
            let n = |k: &str| v[k].as_f64().unwrap_or(0.0) as f32;
            Pos2::new(50.0, 50.0) + Vec2::new(n("x") + 4.0, n("y") + n("height") / 2.0) * a.zoom
        };
        a.session
            .set_nested(hwpcraft_engine::NestedCaret::at(target.clone(), 5), false)?;
        frame(&mut a, click(point));
        frame_after(&mut a, click(point), 0.1);
        assert!(a.error.is_none(), "{:?}", a.error);
        assert_eq!(a.session.nested_selected_text()?, "가나다");
        assert!(a.error.is_none(), "{:?}", a.error);
        Ok(())
    }

    /// Tab and Shift+Tab move between the cells of a nested table.
    #[test]
    fn tab_moves_between_cells_of_a_nested_table() -> anyhow::Result<()> {
        let (mut a, target, point) = nested_table()?;
        frame(&mut a, vec![]);
        frame(&mut a, click(point));
        // The view registers its Tab filter once the canvas has the focus.
        frame(&mut a, vec![]);
        let cell = |a: &HwpApp| match a.session.nested_caret().and_then(|c| c.address.path.last()) {
            Some(ContextStep::TableCell { cell, .. }) => *cell,
            _ => usize::MAX,
        };
        assert_eq!(cell(&a), 0);
        frame(&mut a, vec![key(Key::Tab)]);
        assert!(a.error.is_none(), "{:?}", a.error);
        assert_eq!(cell(&a), 1);
        frame(&mut a, vec![Event::Text("둘".into())]);
        frame(&mut a, vec![key(Key::Tab), key(Key::Tab), key(Key::Tab)]);
        assert_eq!(cell(&a), 3, "stops at the last cell");
        frame(&mut a, vec![key_with(Key::Tab, egui::Modifiers::SHIFT)]);
        assert_eq!(cell(&a), 2);
        let mut second = target.clone();
        if let Some(ContextStep::TableCell { cell, .. }) = second.path.last_mut() {
            *cell = 1;
        }
        assert_eq!(paragraph(&a, &second).text, "둘");
        assert!(a.error.is_none(), "{:?}", a.error);
        Ok(())
    }

    /// Ribbon and toolbar formatting (`command` / `run`) applies to the nested paragraph,
    /// not to the body, while the nested caret is active.
    #[test]
    fn ribbon_formatting_goes_to_the_nested_caret() -> anyhow::Result<()> {
        let (mut a, target, point) = nested_table()?;
        frame(&mut a, vec![]);
        frame(&mut a, click(point));
        frame(&mut a, vec![Event::Text("서식".into())]);
        let body = |a: &HwpApp| {
            let p = &a.session.core.document().sections[0].paragraphs[0];
            (
                p.char_shapes
                    .iter()
                    .map(|c| c.char_shape_id)
                    .collect::<Vec<_>>(),
                p.para_shape_id,
            )
        };
        let before_body = body(&a);
        let shapes = |a: &HwpApp| {
            paragraph(a, &target)
                .char_shapes
                .iter()
                .map(|c| c.char_shape_id)
                .collect::<Vec<_>>()
        };
        let before = shapes(&a);
        let before_para = paragraph(&a, &target).para_shape_id;
        a.command(hwpcraft_engine::Command::Character(json!({"bold": true})));
        assert!(a.error.is_none(), "{:?}", a.error);
        assert!(a.bold);
        assert_ne!(shapes(&a), before, "nested text is bold");
        assert!(a.run("format.paragraph", json!({"alignment": "center"})));
        assert_ne!(paragraph(&a, &target).para_shape_id, before_para);
        assert_eq!(body(&a), before_body, "the body is untouched");
        // Undo from the ribbon keeps the nested caret.
        a.command(hwpcraft_engine::Command::Undo);
        assert!(a.session.nested_caret().is_some());
        assert_eq!(paragraph(&a, &target).para_shape_id, before_para);
        Ok(())
    }

    /// A click on a text box's text puts the caret there (typing and Tab type into it); a
    /// click near its frame selects the box, as in Hangul.
    #[test]
    fn a_text_box_click_edits_its_text_and_its_frame_selects_it() -> anyhow::Result<()> {
        let ctx = egui::Context::default();
        let mut a = HwpApp::with_context(ctx, None, None, false)?;
        a.session.run(
            "shape.insert",
            json!({"kind": "textbox", "width_mm": 60.0, "height_mm": 20.0, "paragraphs": ["글"]}),
        )?;
        let layout = crate::ext_table::cached_page_layout(&mut None, &a.session, 0);
        let frame_box = layout["controls"]
            .as_array()
            .and_then(|v| v.iter().find(|c| c["type"] == "shape"))
            .map(|c| {
                let f = |k: &str| c[k].as_f64().unwrap_or(0.0) as f32;
                Rect::from_min_size(Pos2::new(f("x"), f("y")), Vec2::new(f("w"), f("h")))
            })
            .ok_or_else(|| anyhow::anyhow!("no text box in the layout: {layout}"))?;
        let zoom = a.zoom;
        let screen = |p: Pos2| Pos2::new(50.0, 50.0) + p.to_vec2() * zoom;
        // A point on the text, well inside the frame.
        let inner = frame_box.shrink(8.0);
        let mut text = None;
        'search: for y in (inner.top() as i32..inner.bottom() as i32).step_by(2) {
            for x in (inner.left() as i32..inner.right() as i32).step_by(4) {
                if let Ok(Some(c)) = a.session.hit_nested(0, f64::from(x), f64::from(y))
                    && matches!(c.address.path.last(), Some(ContextStep::TextBox { .. }))
                {
                    text = Some(Pos2::new(x as f32, y as f32));
                    break 'search;
                }
            }
        }
        let text = text.ok_or_else(|| anyhow::anyhow!("no text in {frame_box:?}"))?;
        frame(&mut a, vec![]);
        frame(&mut a, click(screen(text)));
        assert!(a.error.is_none(), "{:?}", a.error);
        assert!(
            matches!(
                a.session.nested_caret().and_then(|c| c.address.path.last()),
                Some(ContextStep::TextBox { .. })
            ),
            "the caret is in the text box"
        );
        assert_eq!(
            a.object_ext_selected_kind(),
            None,
            "the box is not selected"
        );
        frame(&mut a, vec![]);
        frame(
            &mut a,
            vec![key(Key::End), key(Key::Tab), Event::Text("자".into())],
        );
        assert!(a.error.is_none(), "{:?}", a.error);
        let address = a
            .session
            .nested_caret()
            .map(|c| c.address.clone())
            .expect("caret");
        let v = a
            .session
            .run("context.v2.get", json!({ "address": address }))?;
        assert_eq!(v["text"], "글\t자");

        // Near the frame: the box is selected and the nested caret goes.
        frame(&mut a, vec![key(Key::Escape)]);
        let border = Pos2::new(frame_box.left() + 1.5, frame_box.center().y);
        frame(&mut a, click(screen(border)));
        assert_eq!(a.object_ext_selected_kind(), Some("textbox"));
        assert!(a.session.nested_caret().is_none());
        // The text of the selected box takes the caret again.
        frame(&mut a, click(screen(text)));
        assert!(a.session.nested_caret().is_some());
        assert_eq!(a.object_ext_selected_kind(), None);
        assert!(a.error.is_none(), "{:?}", a.error);
        Ok(())
    }
}
