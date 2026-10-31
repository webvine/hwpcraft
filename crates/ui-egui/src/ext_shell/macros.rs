//! 도구 → 스크립트 매크로: record, name, run and manage key macros through the core
//! area's recorder (`Session::start_macro_recording` …, saved in `macros.json`).
//! As in Hangul's key macros, a caret moved with the mouse is not recorded.
use super::*;
use hwpcraft_engine::{MacroStep, SavedMacro};

/// Shown wherever recording is explained.
pub(crate) const MOUSE_NOTE: &str = "마우스로 옮긴 캐럿은 기록하지 않습니다.";
/// Saved macros listed in the 매크로 menu for running at once.
const MENU_MACROS: usize = 9;

/// "매크로 n" with the smallest n no saved macro uses.
pub(crate) fn default_macro_name(saved: &[SavedMacro]) -> String {
    (1..)
        .map(|n| format!("매크로 {n}"))
        .find(|name| !saved.iter().any(|m| m.name == *name))
        .unwrap_or_else(|| "매크로".into())
}

impl HwpApp {
    pub(crate) fn macro_enabled(&self, id: &str) -> Option<bool> {
        let recording = self.session.macro_recording().is_some();
        Some(match id {
            "macro.record.start" | "macro.run" => !recording,
            "macro.record.stop" => recording,
            _ => return None,
        })
    }

    pub(crate) fn macro_disabled_reason(&self, id: &str) -> Option<&'static str> {
        let recording = self.session.macro_recording().is_some();
        match id {
            "macro.record.start" | "macro.run" if recording => {
                Some("매크로를 기록하고 있습니다. 먼저 [매크로 정의 끝]으로 끝내세요.")
            }
            "macro.record.stop" if !recording => Some("매크로를 기록하고 있지 않습니다."),
            _ => None,
        }
    }

    /// Return true when `id` is a macro command and was handled.
    pub(crate) fn macro_dispatch(&mut self, id: &str) -> bool {
        match id {
            "macro.record.start" => {
                let r = self.session.start_macro_recording();
                if self.result(r).is_some() {
                    self.message = format!(
                        "매크로 기록을 시작했습니다. 끝내려면 Alt+Shift+H를 누르세요. {MOUSE_NOTE}"
                    );
                }
            }
            "macro.record.stop" => {
                let r = self.session.stop_macro_recording();
                let Some(steps) = self.result(r) else {
                    return true;
                };
                if steps.is_empty() {
                    self.message = "기록한 동작이 없어 매크로를 만들지 않았습니다.".into();
                } else {
                    self.message = format!(
                        "기록을 끝냈습니다({}단계). 이름을 정해 저장하세요.",
                        steps.len()
                    );
                    let saved = hwpcraft_engine::saved_macros().unwrap_or_default();
                    self.ext.shell.dialog = Some(Dialog::MacroName {
                        name: default_macro_name(&saved),
                        steps,
                    });
                }
            }
            "macro.run" => {
                let list = hwpcraft_engine::saved_macros();
                match self.result(list) {
                    Some(list) if list.is_empty() => {
                        self.message =
                            "저장한 매크로가 없습니다. 먼저 [매크로 정의]로 기록하세요.".into();
                    }
                    Some(list) => {
                        self.ext.shell.dialog = Some(Dialog::Macros {
                            selected: list.len() - 1,
                            list,
                            repeat: 1,
                        })
                    }
                    None => {}
                }
            }
            _ => return false,
        }
        true
    }

    /// Alt+Shift+H (매크로 정의 / 끝) and Alt+Shift+L (매크로 실행). Must run before
    /// Alt+L (글자 모양), which would otherwise take Alt+Shift+L too.
    pub(crate) fn shell_macro_keys(&mut self, ctx: &egui::Context) -> bool {
        use egui::{Key, Modifiers};
        // 배포용 문서: typing, pasting, cutting and copying never reach the document;
        // say why once instead of an error for every key.
        if self.session.is_distribution() {
            let blocked = ctx.input_mut(|i| {
                let before = i.events.len();
                i.events.retain(|e| {
                    !matches!(
                        e,
                        egui::Event::Text(_)
                            | egui::Event::Paste(_)
                            | egui::Event::Cut
                            | egui::Event::Copy
                            | egui::Event::Ime(_)
                            | egui::Event::Key {
                                key: Key::Backspace | Key::Delete | Key::Enter | Key::Tab,
                                pressed: true,
                                ..
                            }
                    )
                });
                before != i.events.len()
            });
            if blocked {
                self.message = hwpcraft_engine::DISTRIBUTION_READ_ONLY.into();
            }
        }
        // The user's own shortcuts come before every built-in one.
        if self.user_shortcut_keys(ctx) {
            return true;
        }
        let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
        let id = if ctx.input_mut(|i| i.consume_key(alt_shift, Key::H)) {
            if self.session.macro_recording().is_some() {
                "macro.record.stop"
            } else {
                "macro.record.start"
            }
        } else if ctx.input_mut(|i| i.consume_key(alt_shift, Key::L)) {
            "macro.run"
        } else {
            return false;
        };
        // Alt chords must not also type a letter into the document.
        ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(_))));
        if self.macro_enabled(id) == Some(true) {
            self.macro_dispatch(id);
        } else if let Some(why) = self.macro_disabled_reason(id) {
            self.message = why.into();
        }
        true
    }

    /// Run a saved macro; on failure the core area has already restored the document.
    pub(crate) fn run_saved_macro(&mut self, name: &str, steps: &[MacroStep], repeat: u32) {
        match self.session.run_macro(steps, repeat) {
            Ok(_) => {
                self.message = if repeat > 1 {
                    format!("매크로 [{name}]을(를) {repeat}번 실행했습니다.")
                } else {
                    format!("매크로 [{name}]을(를) 실행했습니다.")
                };
                self.sync_formatting();
                self.follow_cursor();
            }
            Err(e) => {
                self.message =
                    format!("매크로 [{name}] 실행 실패: {e} 문서는 실행 전으로 되돌렸습니다.");
            }
        }
    }

    /// The saved macros at the end of the 매크로 menu (the first nine run at once).
    pub(crate) fn macro_menu_entries(&mut self, ui: &mut egui::Ui) {
        let list = hwpcraft_engine::saved_macros().unwrap_or_default();
        if list.is_empty() {
            return;
        }
        ui.separator();
        let enabled = self.session.macro_recording().is_none();
        for (n, m) in list.iter().rev().take(MENU_MACROS).enumerate() {
            let label = format!("{}  {}  ({}단계)", n + 1, m.name, m.steps.len());
            let response = ui.add_enabled(enabled, egui::Button::new(label).frame(false));
            if response.clicked() {
                ui.close();
                self.run_saved_macro(&m.name, &m.steps, 1);
            }
        }
    }

    pub(crate) fn macro_name_dialog(
        &mut self,
        ctx: &egui::Context,
        name: &mut String,
        steps: &[MacroStep],
    ) -> bool {
        let mut focus = false;
        let button = dialog(
            ctx,
            "매크로 정의",
            420.0,
            &["저장(D)", "취소"],
            |ui| {
                group(ui, "매크로 이름");
                let r = ui.add(egui::TextEdit::singleline(name).desired_width(240.0));
                focus = !r.has_focus() && ui.memory(|m| m.focused().is_none());
                if focus {
                    r.request_focus();
                }
                ui.add_space(6.0);
                ui.label(format!("기록한 동작 {}단계", steps.len()));
                notice(
                    ui,
                    &format!(
                        "같은 이름의 매크로는 바뀝니다. 취소하면 기록을 버립니다. {MOUSE_NOTE}"
                    ),
                );
            },
        );
        // Enter in the name box saves too.
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        match button {
            Some(0) => {}
            None if enter && !name.trim().is_empty() => {}
            Some(_) => {
                self.message = "매크로 기록을 버렸습니다.".into();
                return false;
            }
            None => return true,
        }
        let name = name.trim().to_owned();
        match hwpcraft_engine::save_macro(&name, steps.to_vec()) {
            Ok(()) => {
                self.message = format!(
                    "매크로 [{name}]을(를) 저장했습니다({}단계). Alt+Shift+L로 실행합니다.",
                    steps.len()
                );
                false
            }
            Err(e) => {
                // Keep the dialog so the name can be fixed.
                self.message = format!("매크로를 저장하지 못했습니다: {e}");
                true
            }
        }
    }

    pub(crate) fn macros_dialog(
        &mut self,
        ctx: &egui::Context,
        list: &mut Vec<SavedMacro>,
        selected: &mut usize,
        repeat: &mut u32,
    ) -> bool {
        let mut run_now = false;
        let button = dialog(
            ctx,
            "매크로 실행",
            460.0,
            &["실행(D)", "지우기", "닫기"],
            |ui| {
                group(ui, "저장한 매크로");
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for (i, m) in list.iter().enumerate().rev() {
                            let r = ui.selectable_label(
                                *selected == i,
                                format!("{}    {}단계", m.name, m.steps.len()),
                            );
                            if r.clicked() {
                                *selected = i;
                            }
                            if r.double_clicked() {
                                run_now = true;
                            }
                        }
                    });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("반복 횟수");
                    ui.add(egui::DragValue::new(repeat).range(1..=100));
                });
                notice(
                    ui,
                    "각 단계는 실행 취소 한 번으로 되돌립니다. 중간에 실패하면 문서를 실행 전으로 되돌립니다.",
                );
            },
        );
        let Some(chosen) = list.get(*selected).cloned() else {
            return button.is_none();
        };
        match button {
            Some(0) => {}
            None if run_now => {}
            Some(1) => {
                match hwpcraft_engine::delete_macro(&chosen.name) {
                    Ok(()) => {
                        self.message = format!("매크로 [{}]을(를) 지웠습니다.", chosen.name);
                        list.remove(*selected);
                        *selected = selected.saturating_sub(1).min(list.len().saturating_sub(1));
                    }
                    Err(e) => self.message = format!("매크로를 지우지 못했습니다: {e}"),
                }
                return !list.is_empty();
            }
            Some(_) => return false,
            None => return true,
        }
        self.run_saved_macro(&chosen.name, &chosen.steps, *repeat);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> anyhow::Result<HwpApp> {
        HwpApp::with_context(egui::Context::default(), None, None, false)
    }

    fn keys(a: &mut HwpApp, key: egui::Key) -> bool {
        let ctx = a.ctx.clone();
        let mut input = egui::RawInput::default();
        let modifiers = egui::Modifiers::ALT | egui::Modifiers::SHIFT;
        input.events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        });
        let mut handled = false;
        let mut output = ctx.run_ui(input, |_| handled = a.shell_macro_keys(&ctx));
        output.textures_delta.clear();
        handled
    }

    #[test]
    fn new_macros_get_the_first_free_number() {
        let saved = |names: &[&str]| -> Vec<SavedMacro> {
            names
                .iter()
                .map(|n| SavedMacro {
                    name: (*n).into(),
                    steps: Vec::new(),
                })
                .collect()
        };
        assert_eq!(default_macro_name(&[]), "매크로 1");
        assert_eq!(
            default_macro_name(&saved(&["매크로 1", "매크로 3"])),
            "매크로 2"
        );
    }

    #[test]
    fn alt_shift_h_records_and_ending_asks_for_a_name() -> anyhow::Result<()> {
        let mut a = app()?;
        assert!(keys(&mut a, egui::Key::H));
        assert_eq!(a.session.macro_recording(), Some(0));
        assert_eq!(a.macro_enabled("macro.run"), Some(false));
        assert!(a.macro_disabled_reason("macro.run").is_some());
        // Running is refused while recording, with the reason on the status bar.
        assert!(keys(&mut a, egui::Key::L));
        assert!(a.ext.shell.dialog.is_none());
        assert!(a.message.contains("기록하고 있습니다"), "{}", a.message);
        a.session.execute(Command::Insert("머리말".into()))?;
        assert!(keys(&mut a, egui::Key::H));
        assert_eq!(a.session.macro_recording(), None);
        let Some(Dialog::MacroName { name, steps }) = &a.ext.shell.dialog else {
            panic!("no name dialog");
        };
        assert!(name.starts_with("매크로 "));
        assert_eq!(steps.len(), 1);
        Ok(())
    }

    #[test]
    fn an_empty_recording_makes_no_macro() -> anyhow::Result<()> {
        let mut a = app()?;
        assert!(a.macro_dispatch("macro.record.start"));
        assert!(a.macro_dispatch("macro.record.stop"));
        assert!(a.ext.shell.dialog.is_none());
        assert!(a.message.contains("만들지 않았습니다"), "{}", a.message);
        Ok(())
    }

    #[test]
    fn a_failing_macro_says_so_and_leaves_the_document_as_it_was() -> anyhow::Result<()> {
        let mut a = app()?;
        a.session.execute(Command::Insert("원문".into()))?;
        let before = a.session.text().to_owned();
        let steps = vec![
            MacroStep {
                command: "text.insert".into(),
                params: serde_json::json!({"text": "추가"}),
            },
            MacroStep {
                command: "no.such.command".into(),
                params: serde_json::json!({}),
            },
        ];
        a.run_saved_macro("깨진 매크로", &steps, 1);
        assert!(a.message.contains("실행 실패"), "{}", a.message);
        assert_eq!(a.session.text().to_owned(), before);
        Ok(())
    }
}
