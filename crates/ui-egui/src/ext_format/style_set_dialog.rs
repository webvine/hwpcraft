//! Owner: format area. 스타일마당 window (서식 > 스타일마당): pick a style set and
//! apply it through the engine's `format.style.set.apply`, or keep the document's
//! styles as a user set (`format.style.set.save`) and remove one again.
use super::super::*;
use serde_json::Value;

pub(crate) struct StyleSetDialog {
    /// `format.style.set.list` sets.
    sets: Vec<Value>,
    selected: usize,
    /// Name for 현재 문서 스타일 저장.
    name: String,
}

enum Act {
    Save,
    Delete(String),
}

impl HwpApp {
    pub(crate) fn open_style_set(&mut self) {
        let Some(sets) = self.style_sets() else {
            return;
        };
        self.ext.format.style_set = Some(StyleSetDialog {
            sets,
            selected: 0,
            name: String::new(),
        });
    }

    fn style_sets(&mut self) -> Option<Vec<Value>> {
        let r = self.session.run("format.style.set.list", json!({}));
        self.result(r)
            .map(|v| v["sets"].as_array().cloned().unwrap_or_default())
    }

    pub(crate) fn style_set_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.format.style_set.take() else {
            return;
        };
        let mut act = None;
        let clicked = super::super::ext_shell::dialog(
            ctx,
            "스타일마당",
            520.0,
            &["적용(D)", "닫기"],
            |ui| {
                super::super::ext_shell::group(ui, "스타일마당 목록");
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(160.0);
                        egui::ScrollArea::vertical()
                            .id_salt("style-set-list")
                            .max_height(160.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                for (i, set) in d.sets.iter().enumerate() {
                                    let name = set["name"].as_str().unwrap_or_default();
                                    let label = if set["builtin"] == true {
                                        name.to_owned()
                                    } else {
                                        format!("{name} (사용자)")
                                    };
                                    ui.selectable_value(&mut d.selected, i, label);
                                }
                            });
                    });
                    ui.vertical(|ui| {
                        let Some(set) = d.sets.get(d.selected) else {
                            return;
                        };
                        match set["description"].as_str().filter(|t| !t.is_empty()) {
                            Some(text) => ui.label(text),
                            None => ui.label("문서에서 저장한 스타일 묶음입니다."),
                        };
                        let styles: Vec<&str> = set["styles"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .collect();
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(format!(
                                "바꾸는 스타일 {}개: {}",
                                styles.len(),
                                styles.join(", ")
                            ))
                            .size(12.0)
                            .color(Color32::GRAY),
                        );
                        let alt_e =
                            ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::E));
                        if set["builtin"] != true && (ui.button("지우기(E)").clicked() || alt_e)
                        {
                            act = Some(Act::Delete(
                                set["name"].as_str().unwrap_or_default().to_owned(),
                            ));
                        }
                    });
                });
                super::super::ext_shell::group(ui, "현재 문서 스타일 저장");
                ui.horizontal(|ui| {
                    ui.label("이름");
                    let field =
                        ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(200.0));
                    chrome::first_focus(&field);
                    let named = !d.name.trim().is_empty();
                    let alt_s = named
                        && ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::S));
                    if ui
                        .add_enabled(named, egui::Button::new("저장(S)"))
                        .on_disabled_hover_text("저장할 이름을 쓰세요.")
                        .clicked()
                        || alt_s
                    {
                        act = Some(Act::Save);
                    }
                });
                super::super::ext_shell::notice(
                    ui,
                    "적용하면 묶음의 스타일을 이름으로 찾아 고치고, 문서에 없는 스타일은 더합니다. 그 스타일을 쓰는 문단이 함께 바뀌며 한 번에 되돌릴 수 있습니다. 기본 묶음은 HwpCraft가 만든 것으로 글꼴은 그대로 두고 크기·굵기·간격·정렬만 바꿉니다. 한글의 스타일마당 파일은 쓰지 않습니다.",
                );
            },
        );
        match act {
            Some(Act::Save) => {
                let name = d.name.trim().to_owned();
                let r = self
                    .session
                    .run("format.style.set.save", json!({ "name": name }));
                if let Some(done) = self.result(r) {
                    d.sets = done["sets"].as_array().cloned().unwrap_or_default();
                    d.selected = d
                        .sets
                        .iter()
                        .position(|s| s["name"] == name.as_str())
                        .unwrap_or(0);
                    self.message = format!("스타일마당 [{name}]을 저장했습니다.");
                    d.name.clear();
                }
            }
            Some(Act::Delete(name)) => {
                let r = self
                    .session
                    .run("format.style.set.delete", json!({ "name": name }));
                if let Some(done) = self.result(r) {
                    d.sets = done["sets"].as_array().cloned().unwrap_or_default();
                    d.selected = d.selected.min(d.sets.len().saturating_sub(1));
                }
            }
            None => {}
        }
        match clicked {
            Some(0) => {
                if let Some(name) = d.sets.get(d.selected).and_then(|s| s["name"].as_str()) {
                    self.run("format.style.set.apply", json!({ "name": name }));
                }
            }
            Some(_) => {}
            None => self.ext.format.style_set = Some(d),
        }
    }
}
