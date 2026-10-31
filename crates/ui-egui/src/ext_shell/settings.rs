//! 환경 설정 and 사용자 설정 (단축키): settings kept in `view.txt` in the settings folder
//! as `key=value` lines, one `shortcut=<keys>\t<command id>` line per user shortcut.
//! Unknown lines are ignored, so older and newer versions can share the file.
use super::*;

/// Every key a setting may be; `show_unsupported_commands` predates this module.
const AUTOSAVE: &str = "autosave_minutes";
const START_ZOOM: &str = "start_zoom";
const SHORTCUT: &str = "shortcut";
const USER_NAME: &str = "user_name";
const AUTOCORRECT: &str = "autocorrect";
const AUTOCORRECT_OFF: &str = "autocorrect_off";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    pub show_unsupported: bool,
    /// 자동 복구 사본 간격, 0 = off.
    pub autosave_minutes: u32,
    /// Zoom a window starts with, percent.
    pub start_zoom: u32,
    /// (keys like "Ctrl+Alt+K", ribbon command id).
    pub shortcuts: Vec<(String, String)>,
    /// 개인 정보 바꾸기: the user's name, the author of memos made here (empty: 사용자).
    pub user_name: String,
    /// 빠른 교정: whether it applies while typing, and (typed word, replacement) pairs.
    pub autocorrect_enabled: bool,
    pub autocorrect: Vec<(String, String)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_unsupported: false,
            autosave_minutes: 5,
            start_zoom: 90,
            shortcuts: Vec::new(),
            user_name: String::new(),
            autocorrect_enabled: true,
            autocorrect: Vec::new(),
        }
    }
}

pub(crate) const AUTOSAVE_CHOICES: [u32; 5] = [0, 1, 5, 10, 30];

impl Settings {
    pub(crate) fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for (key, value) in text.lines().filter_map(|l| l.split_once('=')) {
            let value = value.trim();
            match key.trim() {
                SHOW_UNSUPPORTED => s.show_unsupported = value == "1",
                AUTOSAVE => {
                    if let Ok(n) = value.parse::<u32>()
                        && AUTOSAVE_CHOICES.contains(&n)
                    {
                        s.autosave_minutes = n;
                    }
                }
                START_ZOOM => {
                    if let Ok(n) = value.parse::<u32>() {
                        s.start_zoom = n.clamp(25, 400);
                    }
                }
                AUTOCORRECT_OFF => s.autocorrect_enabled = value != "1",
                AUTOCORRECT => {
                    if let Some((from, to)) = value.split_once('\t')
                        && super::autocorrect::valid_word(from.trim())
                        && !to.trim().is_empty()
                        && to.trim().chars().count() <= super::autocorrect::MAX_WORD
                        && s.autocorrect.len() < super::autocorrect::MAX_ENTRIES
                    {
                        s.autocorrect
                            .push((from.trim().to_owned(), to.trim().to_owned()));
                    }
                }
                USER_NAME => {
                    s.user_name = value.chars().filter(|c| !c.is_control()).take(40).collect();
                }
                SHORTCUT => {
                    if let Some((keys, id)) = value.split_once('\t')
                        && parse_keys(keys).is_some()
                        && !id.trim().is_empty()
                    {
                        s.shortcuts
                            .push((keys.trim().to_owned(), id.trim().to_owned()));
                    }
                }
                _ => {}
            }
        }
        s
    }

    pub(crate) fn serialize(&self) -> String {
        let mut out = format!(
            "{SHOW_UNSUPPORTED}={}\n{AUTOSAVE}={}\n{START_ZOOM}={}\n",
            u8::from(self.show_unsupported),
            self.autosave_minutes,
            self.start_zoom
        );
        if !self.user_name.is_empty() {
            out.push_str(&format!("{USER_NAME}={}\n", self.user_name));
        }
        if !self.autocorrect_enabled {
            out.push_str(&format!("{AUTOCORRECT_OFF}=1\n"));
        }
        for (from, to) in &self.autocorrect {
            out.push_str(&format!("{AUTOCORRECT}={from}\t{to}\n"));
        }
        for (keys, id) in &self.shortcuts {
            out.push_str(&format!("{SHORTCUT}={keys}\t{id}\n"));
        }
        out
    }

    pub(crate) fn save(&self) {
        if let Some(file) = view_file() {
            if let Some(dir) = file.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(file, self.serialize());
        }
    }
}

/// "Ctrl+Alt+K" → modifiers and key. A user shortcut needs Ctrl or Alt, so plain
/// typing is never taken.
pub(crate) fn parse_keys(keys: &str) -> Option<(egui::Modifiers, egui::Key)> {
    let mut modifiers = egui::Modifiers::NONE;
    let mut key = None;
    for part in keys.split('+').map(str::trim) {
        match part {
            "Ctrl" => modifiers.command = true,
            "Alt" => modifiers.alt = true,
            "Shift" => modifiers.shift = true,
            name => key = Some(egui::Key::from_name(name)?),
        }
    }
    (modifiers.command || modifiers.alt).then_some(())?;
    Some((modifiers, key?))
}

/// Text for a pressed key with its modifiers, as stored.
pub(crate) fn keys_text(modifiers: egui::Modifiers, key: egui::Key) -> String {
    let mut out = String::new();
    if modifiers.command || modifiers.ctrl {
        out.push_str("Ctrl+");
    }
    if modifiers.alt {
        out.push_str("Alt+");
    }
    if modifiers.shift {
        out.push_str("Shift+");
    }
    out.push_str(key.name());
    out
}

impl HwpApp {
    /// User shortcuts, before every built-in one; the most specific first.
    pub(crate) fn user_shortcut_keys(&mut self, ctx: &egui::Context) -> bool {
        if self.ext.shell.settings.shortcuts.is_empty() {
            return false;
        }
        let mut keys: Vec<(egui::Modifiers, egui::Key, String)> = self
            .ext
            .shell
            .settings
            .shortcuts
            .iter()
            .filter_map(|(k, id)| parse_keys(k).map(|(m, key)| (m, key, id.clone())))
            .collect();
        keys.sort_by_key(|(m, _, _)| {
            std::cmp::Reverse(u8::from(m.command) + u8::from(m.alt) + u8::from(m.shift))
        });
        for (modifiers, key, id) in keys {
            if ctx.input_mut(|i| i.consume_key(modifiers, key)) {
                ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(_))));
                if !self.run_ribbon_command(&id, ctx) {
                    self.message = format!("단축키의 명령을 찾지 못했습니다: {id}");
                }
                return true;
            }
        }
        false
    }

    /// 환경 설정.
    pub(crate) fn options_dialog(&mut self, ctx: &egui::Context, draft: &mut Settings) -> bool {
        let button = dialog(ctx, "환경 설정", 460.0, &["설정(D)", "취소"], |ui| {
            group(ui, "편집");
            egui::Grid::new("options-grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("자동 복구 사본");
                    egui::ComboBox::from_id_salt("options-autosave")
                        .selected_text(minutes_label(draft.autosave_minutes))
                        .show_ui(ui, |ui| {
                            for n in AUTOSAVE_CHOICES {
                                ui.selectable_value(
                                    &mut draft.autosave_minutes,
                                    n,
                                    minutes_label(n),
                                );
                            }
                        });
                    ui.end_row();
                    ui.label("처음 확대 비율");
                    ui.add(
                        egui::DragValue::new(&mut draft.start_zoom)
                            .range(25..=400)
                            .suffix(" %"),
                    );
                    ui.end_row();
                });
            group(ui, "화면");
            ui.checkbox(
                &mut draft.show_unsupported,
                "지원하지 않는 명령도 리본에 보이기",
            );
            notice(
                ui,
                "자동 복구 사본은 바뀐 문서를 쉬는 동안 설정 폴더에 저장했다가, 프로그램이 \
                 비정상으로 끝나면 다음 시작 때 복구를 제안합니다. 처음 확대 비율은 새 창부터 적용됩니다.",
            );
        });
        match button {
            Some(0) => {
                if draft.show_unsupported != self.ext.shell.show_unsupported {
                    self.ext.shell.show_unsupported = draft.show_unsupported;
                }
                draft.shortcuts = self.ext.shell.settings.shortcuts.clone();
                self.ext.shell.settings = draft.clone();
                self.ext.shell.settings.save();
                self.message = "환경 설정을 저장했습니다.".into();
                false
            }
            Some(_) => false,
            None => true,
        }
    }

    /// 개인 정보 바꾸기: the name memos made in this app carry as their author. It stays
    /// in the settings folder; documents get it only where a memo is created here.
    pub(crate) fn user_info_dialog(&mut self, ctx: &egui::Context, name: &mut String) -> bool {
        let button = dialog(
            ctx,
            "개인 정보 바꾸기",
            420.0,
            &["설정(D)", "취소"],
            |ui| {
                group(ui, "사용자");
                ui.horizontal(|ui| {
                    ui.label("이름");
                    chrome::first_focus(
                        &ui.add(
                            egui::TextEdit::singleline(name)
                                .char_limit(40)
                                .hint_text("사용자")
                                .desired_width(220.0),
                        ),
                    );
                });
                notice(
                    ui,
                    "새 메모의 작성자로 씁니다. 비워 두면 \"사용자\"입니다. 설정 폴더에만 \
                 저장하고, 문서에는 메모를 만들 때만 들어갑니다. MCP·CLI로 만든 메모는 \
                 명령에 준 작성자를 씁니다.",
                );
            },
        );
        match button {
            Some(0) => {
                self.ext.shell.settings.user_name = name
                    .trim()
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(40)
                    .collect();
                self.ext.shell.settings.save();
                self.message = "개인 정보를 저장했습니다.".into();
                false
            }
            Some(_) => false,
            None => true,
        }
    }

    /// 사용자 설정 → 단축키: a command from the ribbon and a key with Ctrl or Alt.
    pub(crate) fn shortcuts_dialog(
        &mut self,
        ctx: &egui::Context,
        state: &mut ShortcutDraft,
    ) -> bool {
        if state.capturing {
            let pressed = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.command || modifiers.alt => Some(keys_text(*modifiers, *key)),
                    _ => None,
                })
            });
            if let Some(keys) = pressed {
                state.keys = keys;
                state.capturing = false;
                ctx.input_mut(|i| i.events.clear());
            }
        }
        let commands = super::super::ribbon::ribbon_commands();
        let mut remove = None;
        let mut add = false;
        let label_of = |id: &str| {
            commands
                .iter()
                .find(|(_, c)| c == id)
                .map_or(id.to_owned(), |(l, _)| l.clone())
        };
        let taken_by = (!state.keys.is_empty())
            .then(|| shortcut_owner(&state.keys))
            .flatten();
        let button = dialog(
            ctx,
            "사용자 설정 · 단축키",
            560.0,
            &["닫기"],
            |ui| {
                group(ui, "내 단축키");
                if self.ext.shell.settings.shortcuts.is_empty() {
                    ui.label(
                        RichText::new("만든 단축키가 없습니다.").color(Color32::from_gray(120)),
                    );
                }
                egui::Grid::new("user-shortcuts")
                    .num_columns(3)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        for (i, (keys, id)) in self.ext.shell.settings.shortcuts.iter().enumerate()
                        {
                            ui.label(keys);
                            ui.label(label_of(id));
                            if ui.small_button("지우기").clicked() {
                                remove = Some(i);
                            }
                            ui.end_row();
                        }
                    });
                group(ui, "새 단축키");
                ui.horizontal(|ui| {
                    ui.label("명령");
                    egui::ComboBox::from_id_salt("shortcut-command")
                        .width(240.0)
                        .selected_text(if state.command.is_empty() {
                            "명령을 고르세요".to_owned()
                        } else {
                            label_of(&state.command)
                        })
                        .show_ui(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(300.0)
                                .show(ui, |ui| {
                                    for (label, id) in &commands {
                                        ui.selectable_value(&mut state.command, id.clone(), label);
                                    }
                                });
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("키");
                    let text = if state.capturing {
                        "Ctrl이나 Alt와 함께 키를 누르세요…".to_owned()
                    } else if state.keys.is_empty() {
                        "키 누르기".to_owned()
                    } else {
                        state.keys.clone()
                    };
                    if ui.button(text).clicked() {
                        state.capturing = true;
                    }
                    add = ui
                        .add_enabled(
                            !state.command.is_empty() && !state.keys.is_empty(),
                            egui::Button::new("넣기"),
                        )
                        .clicked();
                });
                if let Some(owner) = &taken_by {
                    ui.label(
                        RichText::new(format!("기본 단축키 [{owner}] 대신 이 명령이 실행됩니다."))
                            .color(Color32::from_rgb(170, 90, 0)),
                    );
                }
                notice(
                    ui,
                    "단축키는 Ctrl이나 Alt를 포함해야 합니다. 같은 키의 기본 단축키보다 먼저 실행됩니다.",
                );
            },
        );
        let mut changed = false;
        if let Some(i) = remove {
            self.ext.shell.settings.shortcuts.remove(i);
            changed = true;
        }
        if add {
            let keys = state.keys.clone();
            self.ext
                .shell
                .settings
                .shortcuts
                .retain(|(k, _)| *k != keys);
            self.ext
                .shell
                .settings
                .shortcuts
                .push((keys.clone(), state.command.clone()));
            self.message = format!("{keys}: {}", label_of(&state.command));
            state.keys.clear();
            changed = true;
        }
        if changed {
            self.ext.shell.settings.show_unsupported = self.ext.shell.show_unsupported;
            self.ext.shell.settings.save();
        }
        button.is_none()
    }
}

/// The built-in command already using `keys`, if any (by the command list labels).
fn shortcut_owner(keys: &str) -> Option<String> {
    hwpcraft_engine::COMMANDS
        .iter()
        .chain(hwpcraft_engine::extension_commands())
        .find(|(_, _, k)| k.split(" / ").any(|k| k.trim() == keys))
        .map(|(_, label, _)| (*label).to_owned())
}

fn minutes_label(n: u32) -> String {
    if n == 0 {
        "쓰지 않음".into()
    } else {
        format!("{n}분마다")
    }
}

/// 단축키 창 입력 상태.
#[derive(Default)]
pub(crate) struct ShortcutDraft {
    pub command: String,
    pub keys: String,
    pub capturing: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_ignore_what_they_do_not_know() {
        let s = Settings {
            show_unsupported: true,
            autosave_minutes: 10,
            start_zoom: 120,
            shortcuts: vec![("Ctrl+Alt+K".into(), "table.create".into())],
            user_name: "김검토".into(),
            autocorrect_enabled: false,
            autocorrect: vec![("ㄱㅅ".into(), "감사합니다".into())],
        };
        let text = s.serialize();
        assert_eq!(Settings::parse(&text), s);
        // The old one-line file still reads; junk and bad values are skipped.
        let old = Settings::parse("show_unsupported_commands=1\n");
        assert!(old.show_unsupported && old.autosave_minutes == 5);
        let odd = Settings::parse(
            "autosave_minutes=7\nstart_zoom=9000\nshortcut=K\ttable.create\nwhatever=1\n",
        );
        assert_eq!((odd.autosave_minutes, odd.start_zoom), (5, 400));
        assert!(
            odd.shortcuts.is_empty(),
            "a shortcut without Ctrl or Alt is ignored"
        );
    }

    #[test]
    fn keys_parse_and_print_the_same_way() {
        let (m, k) = parse_keys("Ctrl+Shift+F5").expect("keys");
        assert!(m.command && m.shift && !m.alt && k == egui::Key::F5);
        assert_eq!(keys_text(m, k), "Ctrl+Shift+F5");
        assert!(parse_keys("Shift+A").is_none());
        assert!(parse_keys("Ctrl+Nope").is_none());
        assert!(
            shortcut_owner("Ctrl+B").is_some(),
            "a built-in key is reported"
        );
        assert_eq!(shortcut_owner("Ctrl+Alt+Shift+F12"), None);
    }

    #[test]
    fn the_name_field_takes_the_keyboard_from_the_document() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        let ctx = a.ctx.clone();
        let canvas = egui::Id::new("document-canvas");
        ctx.memory_mut(|m| m.request_focus(canvas));
        let mut name = String::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            a.user_info_dialog(ui.ctx(), &mut name);
        });
        output.textures_delta.clear();
        let focused = ctx.memory(|m| m.focused());
        assert!(focused.is_some_and(|id| id != canvas), "{focused:?}");
        Ok(())
    }

    #[test]
    fn the_user_name_is_the_author_of_new_memos() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        a.ext.shell.settings.user_name = "김검토".into();
        let ctx = a.ctx.clone();
        a.comments_ext_dispatch("memo.create", &ctx);
        assert_eq!(a.ext.comments.author, "김검토");
        Ok(())
    }

    #[test]
    fn a_user_shortcut_runs_its_ribbon_command_first() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        a.ext.shell.settings.shortcuts = vec![("Ctrl+Alt+G".into(), "view.grid".into())];
        let ctx = a.ctx.clone();
        let mut handled = false;
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::G,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND | egui::Modifiers::ALT,
                }],
                ..Default::default()
            },
            |_| handled = a.user_shortcut_keys(&ctx),
        );
        output.textures_delta.clear();
        assert!(handled && a.ext.shell.show_grid);
        Ok(())
    }
}
