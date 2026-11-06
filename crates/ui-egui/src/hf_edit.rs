//! Typing on the page inside a header or footer (머리말·꼬리말 편집). The shell enters
//! and leaves the area (`hf_shell`); while the engine has an entered header or footer,
//! canvas events come here and edit it. Ribbon and toolbar formatting goes to it too
//! (`hf_command`).
use crate::{HwpApp, clipboard};
use egui::{Event, ImeEvent, Key};
use hwpcraft_engine::{Command, Motion};
use serde_json::{Value, json};

impl HwpApp {
    /// Handle one canvas event while a header or footer is entered; `false` leaves it
    /// to the other handlers (Esc to leave, file and window shortcuts).
    pub(crate) fn hf_event(&mut self, ctx: &egui::Context, event: &Event) -> bool {
        let edited = match event {
            Event::Text(t) if self.preedit.is_empty() => self.session.hf_insert(t),
            Event::Ime(ImeEvent::Preedit { text, .. }) => {
                self.preedit = text.clone();
                return true;
            }
            Event::Ime(ImeEvent::Commit(text)) => {
                self.preedit.clear();
                self.session.hf_insert(text)
            }
            Event::Copy | Event::Cut => {
                if let Ok(text) = self.session.hf_selected_text()
                    && !text.is_empty()
                    && !clipboard::set(&text, None)
                {
                    ctx.copy_text(text);
                }
                if matches!(event, Event::Cut) {
                    self.session.hf_insert("")
                } else {
                    return true;
                }
            }
            Event::Paste(text) => self.session.hf_insert(text),
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command => match key {
                // The shell's frame check (hf_frame) leaves a header undo removed.
                Key::Z => {
                    self.command(if modifiers.shift {
                        Command::Redo
                    } else {
                        Command::Undo
                    });
                    return true;
                }
                // Toggled from the header's own format, not the body caret's.
                Key::B | Key::I | Key::U => {
                    let state = self.session.hf_char_state().unwrap_or_default();
                    let (name, on) = match key {
                        Key::B => ("bold", state.bold),
                        Key::I => ("italic", state.italic),
                        _ => ("underline", state.underline),
                    };
                    self.session.hf_character(json!({ (name): !on }))
                }
                Key::A => {
                    self.session.hf_select_all();
                    Ok(())
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
                Key::Escape => return false,
                Key::Enter => self.session.hf_enter_key(),
                Key::Tab => self.session.hf_insert("\t"),
                Key::Backspace => self.session.hf_backspace(),
                Key::Delete => self.session.hf_delete(),
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
                    self.session.hf_move(motion, modifiers.shift)
                }
                // Other keys must not reach the body caret while a header is entered.
                _ => return true,
            },
            _ => return false,
        };
        self.result(edited);
        self.scroll_to_caret = true;
        true
    }

    /// Character or paragraph formatting from the ribbon, the toolbar or `format.*`
    /// while a header or footer is entered: it applies there (its selection, or the
    /// caret paragraph), not to the body. `None` leaves the command to the body.
    pub(crate) fn hf_command(&mut self, command: &Command) -> Option<bool> {
        self.session.hf_target()?;
        // The body's format state (self.bold, …) stays the body caret's.
        let r = match command {
            Command::Character(format) => self.session.hf_character(format.clone()),
            Command::Paragraph(format) => self.session.hf_paragraph_format(format.clone()),
            Command::Undo | Command::Redo => self
                .session
                .execute(if matches!(command, Command::Undo) {
                    Command::Undo
                } else {
                    Command::Redo
                })
                .map(|_| ()),
            _ => return None,
        };
        Some(self.result(r).is_some())
    }

    /// `format.character` / `format.paragraph` run by id for `hf_command`, and
    /// `page.header_footer.field` (쪽 번호 넣기 and the other fields) at the caret.
    pub(crate) fn hf_run(&mut self, id: &str, params: &Value) -> Option<bool> {
        self.session.hf_target()?;
        match id {
            "format.character" => self.hf_command(&Command::Character(params.clone())),
            "format.paragraph" => self.hf_command(&Command::Paragraph(params.clone())),
            "page.header_footer.field" => {
                let kind = params["kind"].as_str().unwrap_or("page_number");
                let r = self.session.hf_insert_field(kind);
                Some(self.result(r).is_some())
            }
            // Row, column and merge edits go to the body caret's table: never while a
            // header or footer is edited (a header table's own come with its cell caret).
            "table.edit" => {
                self.message = crate::hf_objects::HF_OBJECT_UNSUPPORTED.into();
                Some(false)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::HwpApp;
    use egui::{Event, ImeEvent, Key};
    use hwpcraft_engine::{Command, HfApply, HfTarget};
    use serde_json::json;

    fn key(key: Key, shift: bool) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: if shift {
                egui::Modifiers::SHIFT
            } else {
                egui::Modifiers::NONE
            },
        }
    }

    /// Keys and the input method edit the entered header and footer, and Esc is left
    /// to the shell.
    #[test]
    fn keys_and_ime_edit_the_entered_header_or_footer() {
        for footer in [false, true] {
            let mut a = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
            let ctx = a.ctx.clone();
            a.session.execute(Command::Insert("본문".into())).unwrap();
            let body = a.session.run("document.text", json!({})).unwrap();
            let target = HfTarget {
                section: 0,
                footer,
                apply_to: HfApply::Both,
            };
            a.session.hf_enter(target, 0).unwrap();
            for event in [
                Event::Text("머리".into()),
                Event::Ime(ImeEvent::Preedit {
                    text: "ㅁ".into(),
                    active_range_chars: None,
                }),
                Event::Ime(ImeEvent::Commit("말".into())),
                key(Key::ArrowLeft, true),
                key(Key::Backspace, false),
                Event::Text("글".into()),
            ] {
                assert!(a.hf_event(&ctx, &event), "{event:?}");
            }
            let got = a
                .session
                .run("page.header_footer.text.get", json!({"header": !footer}))
                .unwrap();
            assert_eq!(got["text"], "머리글", "footer {footer}: {got}");
            assert!(!a.hf_event(&ctx, &key(Key::Escape, false)));
            assert_eq!(a.session.hf_target(), Some(target));
            assert_eq!(a.session.run("document.text", json!({})).unwrap(), body);
            // Bold from the ribbon goes to the header paragraph, not the body.
            assert_eq!(
                a.hf_command(&Command::Character(json!({"bold": true}))),
                Some(true)
            );
            // The header takes it; the body caret's format state stays.
            assert!(a.session.hf_char_state().is_some_and(|c| c.bold));
            assert!(!a.bold);
            // Ctrl+B toggles from the header's format: off again.
            let ctrl_b = Event::Key {
                key: Key::B,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            };
            assert!(a.hf_event(&ctx, &ctrl_b));
            assert!(a.session.hf_char_state().is_some_and(|c| !c.bold));
            assert!(a.hf_event(&ctx, &ctrl_b));
            assert!(a.session.hf_char_state().is_some_and(|c| c.bold));
            // Ctrl+A selects the whole header or footer.
            let ctrl_a = Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            };
            assert!(a.hf_event(&ctx, &ctrl_a));
            assert_eq!(a.session.hf_selected_text().unwrap(), "머리글");
            assert_eq!(
                a.hf_run("page.header_footer.field", &json!({"kind": "page_number"})),
                Some(true)
            );
            let got = a
                .session
                .run("page.header_footer.text.get", json!({"header": !footer}))
                .unwrap();
            assert_eq!(got["fields"][0]["kind"], "page_number", "{got}");
            a.session.hf_exit();
            assert_eq!(
                a.hf_command(&Command::Undo),
                None,
                "the body takes it again"
            );
        }
    }
}
