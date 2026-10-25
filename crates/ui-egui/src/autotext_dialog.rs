//! Owner: review-tools area. Hangul's 상용구 windows: 상용구 내용 (Ctrl+F3: list, preview,
//! 넣기 / 등록 / 지우기) and 상용구 등록 (name, 준말, text from the selection).
//! Other areas must not edit this file; request changes through the owner.
use super::*;
use serde_json::Value;

#[derive(Clone, Default)]
pub(crate) struct Entry {
    pub name: String,
    pub abbreviation: String,
    pub text: String,
}
pub(crate) enum AutoTextDialog {
    Contents {
        entries: Vec<Entry>,
        selected: Option<usize>,
        status: String,
    },
    Register {
        name: String,
        abbreviation: String,
        text: String,
        status: String,
    },
}

fn entries_from(v: &Value) -> Vec<Entry> {
    v["entries"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|e| Entry {
                    name: e["name"].as_str().unwrap_or("").to_owned(),
                    abbreviation: e["abbreviation"].as_str().unwrap_or("").to_owned(),
                    text: e["text"].as_str().unwrap_or("").to_owned(),
                })
                .collect()
        })
        .unwrap_or_default()
}

impl HwpApp {
    /// 상용구 내용 (Ctrl+F3).
    pub(crate) fn open_autotext_contents(&mut self) {
        let r = self.session.run("tools.autotext.list", json!({}));
        let Some(v) = self.result(r) else {
            return;
        };
        let entries = entries_from(&v);
        self.ext.tools.autotext = Some(AutoTextDialog::Contents {
            selected: (!entries.is_empty()).then_some(0),
            entries,
            status: String::new(),
        });
    }
    /// 상용구 등록: seeded with the selected text.
    pub(crate) fn open_autotext_register(&mut self) {
        let text = self.session.selected_text();
        self.ext.tools.autotext = Some(AutoTextDialog::Register {
            name: String::new(),
            abbreviation: String::new(),
            text,
            status: String::new(),
        });
    }
    /// 상용구 바로 넣기 (Alt+I): the abbreviation before the caret becomes the text.
    pub(crate) fn expand_autotext(&mut self) {
        let r = self.session.run("tools.autotext.expand", json!({}));
        match r {
            Ok(v) => {
                self.message = format!(
                    "상용구 '{}'을(를) 넣었습니다.",
                    v["expanded"].as_str().unwrap_or("")
                );
                self.sync_formatting();
                self.follow_cursor();
            }
            Err(e) => self.message = format!("{e:#}"),
        }
    }

    pub(crate) fn autotext_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.ext.tools.autotext.take() else {
            return;
        };
        let keep = match &mut dialog {
            AutoTextDialog::Contents {
                entries,
                selected,
                status,
            } => self.autotext_contents(ctx, entries, selected, status),
            AutoTextDialog::Register {
                name,
                abbreviation,
                text,
                status,
            } => self.autotext_register(ctx, name, abbreviation, text, status),
        };
        if keep && self.ext.tools.autotext.is_none() {
            self.ext.tools.autotext = Some(dialog);
        }
        if !keep {
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("document-canvas")));
        }
    }

    fn autotext_contents(
        &mut self,
        ctx: &egui::Context,
        entries: &mut Vec<Entry>,
        selected: &mut Option<usize>,
        status: &mut String,
    ) -> bool {
        // Enter or a double-click inserts the selected entry, like 넣기.
        let mut activate = false;
        let button = ext_shell::dialog(
            ctx,
            "상용구 내용",
            620.0,
            &["넣기(D)", "등록...", "지우기", "본문 상용구...", "닫기"],
            |ui| {
                ext_shell::group(ui, &format!("상용구 목록 ({}개)", entries.len()));
                if !entries.is_empty() {
                    let (up, down, enter) = ui.input(|i| {
                        (
                            i.key_pressed(Key::ArrowUp),
                            i.key_pressed(Key::ArrowDown),
                            i.key_pressed(Key::Enter),
                        )
                    });
                    let last = entries.len() - 1;
                    if up {
                        *selected = Some(selected.map_or(0, |i| i.saturating_sub(1)));
                    }
                    if down {
                        *selected = Some(selected.map_or(0, |i| (i + 1).min(last)));
                    }
                    activate |= enter && selected.is_some();
                }
                egui::ScrollArea::vertical()
                    .id_salt("autotext-list")
                    .max_height(180.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Grid::new("autotext-entries")
                            .num_columns(2)
                            .striped(true)
                            .spacing([16.0, 4.0])
                            .show(ui, |ui| {
                                ui.strong("이름");
                                ui.strong("준말");
                                ui.end_row();
                                if entries.is_empty() {
                                    ui.label(
                                        RichText::new(
                                            "등록된 상용구가 없습니다. 글을 선택하고 [등록...]을 누르세요.",
                                        )
                                        .color(Color32::GRAY),
                                    );
                                    ui.end_row();
                                }
                                for (i, e) in entries.iter().enumerate() {
                                    let r = ui.selectable_label(*selected == Some(i), &e.name);
                                    if r.clicked() {
                                        *selected = Some(i);
                                    }
                                    if r.double_clicked() {
                                        *selected = Some(i);
                                        activate = true;
                                    }
                                    ui.label(&e.abbreviation);
                                    ui.end_row();
                                }
                            });
                    });
                ext_shell::group(ui, "미리 보기");
                let preview = selected
                    .and_then(|i| entries.get(i))
                    .map(|e| e.text.clone())
                    .unwrap_or_default();
                egui::ScrollArea::vertical()
                    .id_salt("autotext-preview")
                    .max_height(110.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(preview).size(12.0)).wrap());
                    });
                ext_shell::note(
                    ui,
                    "글 상용구만 저장합니다(서식·표·개체 제외). 준말을 입력한 뒤 Alt+I로 바로 넣을 수 있습니다. ↑↓로 고르고 Enter나 두 번 눌러 넣습니다.",
                );
                if !status.is_empty() {
                    ui.label(
                        RichText::new(status.as_str())
                            .small()
                            .color(Color32::DARK_RED),
                    );
                }
            },
        );
        let button = if activate { Some(0) } else { button };
        match button {
            Some(0) => {
                let Some(e) = selected.and_then(|i| entries.get(i)) else {
                    *status = "넣을 상용구를 고르세요.".into();
                    return true;
                };
                let name = e.name.clone();
                if self.run("tools.autotext.insert", json!({"name": name})) {
                    self.message = "상용구를 넣었습니다.".into();
                    return false;
                }
                true
            }
            Some(1) => {
                self.open_autotext_register();
                false
            }
            Some(2) => {
                let Some(e) = selected.and_then(|i| entries.get(i)) else {
                    *status = "지울 상용구를 고르세요.".into();
                    return true;
                };
                let name = e.name.clone();
                let r = self
                    .session
                    .run("tools.autotext.delete", json!({"name": name}));
                if let Some(v) = self.result(r) {
                    *entries = entries_from(&v);
                    *selected = (!entries.is_empty()).then_some(0);
                }
                true
            }
            Some(3) => {
                self.open_body_autotext();
                false
            }
            Some(_) => false,
            None => true,
        }
    }

    fn autotext_register(
        &mut self,
        ctx: &egui::Context,
        name: &mut String,
        abbreviation: &mut String,
        text: &mut String,
        status: &mut String,
    ) -> bool {
        let mut submit = false;
        let abbreviation_ok = abbreviation.chars().count() <= 32
            && !abbreviation
                .chars()
                .any(|c| c.is_whitespace() || c.is_control());
        let button = ext_shell::dialog(
            ctx,
            "상용구 등록",
            560.0,
            &["등록(D)", "취소"],
            |ui| {
                egui::Grid::new("autotext-register")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("이름");
                        let n = ui.add(
                            egui::TextEdit::singleline(name)
                                .id(egui::Id::new("autotext-name"))
                                .desired_width(260.0),
                        );
                        ui.end_row();
                        ui.label("준말");
                        let a = ui.add(
                            egui::TextEdit::singleline(abbreviation)
                                .id(egui::Id::new("autotext-abbr"))
                                .desired_width(160.0)
                                .hint_text("Alt+I로 넣을 때 쓸 짧은 말 (선택)"),
                        );
                        ui.end_row();
                        // Enter in either one-line field registers, as 등록(D) does.
                        submit = (n.lost_focus() || a.lost_focus())
                            && ui.input(|i| i.key_pressed(Key::Enter));
                    });
                ext_shell::group(ui, "내용");
                egui::ScrollArea::vertical()
                    .id_salt("autotext-text")
                    .max_height(140.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(text)
                                .id(egui::Id::new("autotext-body"))
                                .desired_width(f32::INFINITY)
                                .desired_rows(5),
                        );
                    });
                if !abbreviation_ok {
                    ui.label(
                        RichText::new("준말은 공백 없이 32자 이하로 입력하세요.")
                            .color(Color32::DARK_RED),
                    );
                }
                ext_shell::note(
                    ui,
                    "선택한 글이 내용으로 들어갑니다. 줄 바꿈은 넣을 때 문단 나누기가 됩니다.",
                );
                if !status.is_empty() {
                    ui.label(
                        RichText::new(status.as_str())
                            .small()
                            .color(Color32::DARK_RED),
                    );
                }
            },
        );
        let button = if submit { Some(0) } else { button };
        match button {
            Some(0) => {
                if name.trim().is_empty() {
                    *status = "이름을 입력하세요.".into();
                    return true;
                }
                if text.is_empty() {
                    *status = "내용이 비어 있습니다.".into();
                    return true;
                }
                let r = self.session.run(
                    "tools.autotext.add",
                    json!({"name": name.trim(), "abbreviation": abbreviation.trim(), "text": text}),
                );
                match r {
                    Ok(_) => {
                        self.message = format!("상용구 '{}'을(를) 등록했습니다.", name.trim());
                        false
                    }
                    Err(e) => {
                        *status = format!("{e:#}");
                        true
                    }
                }
            }
            Some(_) => false,
            None => true,
        }
    }
}
