//! Owner: review-tools area. Hangul's modeless 찾기 / 찾아 바꾸기 windows (Ctrl+F / Ctrl+H)
//! with the 선택 사항 (띄어쓰기 무시, 아무개 문자, 조건식, 여러 단어, 한글로 한자) and 서식 찾기 options.
use super::*;
use hwpcraft_engine::{FindDirection, FindOptions};

#[derive(Default)]
pub(crate) struct FindState {
    pub replace: bool,
    pub options: FindOptions,
    pub status: String,
    /// The status line reports a result (gray) rather than a problem (red).
    pub status_ok: bool,
    pub focus: bool,
}

/// Counting stops here so a very common query stays responsive.
const COUNT_LIMIT: usize = 10_000;

impl HwpApp {
    /// Open 찾기 or 찾아 바꾸기, seeding the query from a short single-paragraph selection.
    pub(crate) fn open_find(&mut self, replace: bool) {
        self.show_find = true;
        self.find_state.replace = replace;
        self.find_state.status.clear();
        self.find_state.status_ok = false;
        self.find_state.focus = true;
        if self.session.span_anchor.is_none() {
            let selected = self.session.selected_text();
            if !selected.is_empty() && selected.chars().count() <= 200 {
                self.find = selected;
            }
        }
    }
    /// Status line after a hit: how many matches the document has in all.
    fn found_status(&mut self) {
        let count = self
            .session
            .count_matches(&self.find, self.find_state.options, COUNT_LIMIT)
            .unwrap_or(0);
        self.find_state.status = if count >= COUNT_LIMIT {
            format!("찾았습니다. 문서에 {COUNT_LIMIT}곳 이상 있습니다.")
        } else {
            format!("찾았습니다. 문서에 모두 {count}곳 있습니다.")
        };
        self.find_state.status_ok = true;
    }
    fn not_found_status(&mut self) {
        self.find_state.status = "찾는 내용이 없습니다.".into();
        self.find_state.status_ok = false;
    }
    fn find_step(&mut self) {
        let o = self.find_state.options;
        let r = if self.find_state.replace {
            self.session.replace_next(&self.find, &self.replacement, o)
        } else {
            self.session.find(&self.find, o)
        };
        if let Some(found) = self.result(r) {
            if found {
                self.found_status();
            } else {
                self.not_found_status();
            }
            self.sync_formatting();
            self.follow_cursor();
        }
        // After a whole-document search continue downwards, as Hangul does.
        if self.find_state.options.direction == FindDirection::All {
            self.find_state.options.direction = FindDirection::Down;
        }
    }
    pub(crate) fn find_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_find {
            return;
        }
        let replace = self.find_state.replace;
        let mut next = false;
        let mut all = false;
        let mut close = false;
        egui::Window::new(if replace {
            "찾아 바꾸기"
        } else {
            "찾기"
        })
        .id(egui::Id::new("hwp-find-window"))
        // Same light-blue title strip as the other dialogs (chrome::title).
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        // Below the ribbon, format bar and ruler, clear of the vertical scroll bar.
        .default_pos(ctx.content_rect().right_top() + Vec2::new(-500.0, 190.0))
        .frame(
            egui::Frame::window(&ctx.global_style())
                .fill(chrome::Tokens::PANEL)
                .stroke(Stroke::new(1.0, chrome::Tokens::BORDER))
                .corner_radius(0),
        )
        .show(ctx, |ui| {
            ui.set_width(470.0);
            close |= chrome::title(
                ui,
                if replace {
                    "찾아 바꾸기"
                } else {
                    "찾기"
                },
            );
            // Same bordered white inputs as the other HwpCraft dialogs.
            let v = ui.visuals_mut();
            v.extreme_bg_color = chrome::Tokens::INPUT;
            v.widgets.inactive.bg_stroke = Stroke::new(1.0, chrome::Tokens::BORDER);
            v.widgets.inactive.weak_bg_fill = Color32::WHITE;
            v.widgets.inactive.bg_fill = Color32::WHITE;
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(340.0);
                    egui::Grid::new("find-fields")
                        .num_columns(2)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("찾을 내용");
                            let query = ui.add(
                                egui::TextEdit::singleline(&mut self.find)
                                    .id(egui::Id::new("find-query"))
                                    .desired_width(220.0),
                            );
                            if std::mem::take(&mut self.find_state.focus) {
                                query.request_focus();
                            }
                            if query.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                next = true;
                            }
                            ui.end_row();
                            if replace {
                                ui.label("바꿀 내용");
                                let with = ui.add(
                                    egui::TextEdit::singleline(&mut self.replacement)
                                        .id(egui::Id::new("find-replacement"))
                                        .desired_width(220.0),
                                );
                                // Enter in 바꿀 내용 runs 바꾸기, as Enter in 찾을 내용 does.
                                if with.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                    next = true;
                                }
                                ui.end_row();
                            }
                        });
                    chrome::section(ui, "선택 사항");
                    let o = &mut self.find_state.options;
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut o.case_sensitive, "대소문자 구별");
                        ui.checkbox(&mut o.whole_word, "온전한 낱말");
                        ui.checkbox(&mut o.ignore_spaces, "띄어쓰기 무시");
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.add_enabled(
                            !o.regex,
                            egui::Checkbox::new(&mut o.wildcards, "아무개 문자(?, *)"),
                        )
                        .on_disabled_hover_text(
                            "조건식(정규식)을 쓰는 동안에는 ?와 *가 정규식 문법입니다.",
                        );
                        ui.checkbox(&mut o.regex, "조건식(정규식)");
                        ui.checkbox(&mut o.any_word, "여러 단어 찾기");
                        ui.checkbox(&mut o.hanja_by_hangul, "한글로 한자 찾기");
                    });
                    chrome::section(ui, "서식 찾기 (글자 모양)");
                    ui.horizontal_wrapped(|ui| {
                        for (value, label) in [
                            (&mut o.bold, "진하게"),
                            (&mut o.italic, "기울임"),
                            (&mut o.underline, "밑줄"),
                        ] {
                            egui::ComboBox::from_id_salt(("find-format", label))
                                .width(96.0)
                                .selected_text(match value {
                                    None => format!("{label}: 무관"),
                                    Some(true) => format!("{label}: 있음"),
                                    Some(false) => format!("{label}: 없음"),
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(value, None, "무관");
                                    ui.selectable_value(value, Some(true), "있음");
                                    ui.selectable_value(value, Some(false), "없음");
                                });
                        }
                    });
                    ui.horizontal(|ui| {
                        let mut size = o.size_pt.unwrap_or(0);
                        ui.label("크기");
                        if ui
                            .add(
                                egui::DragValue::new(&mut size)
                                    .range(0..=4096)
                                    .suffix(" pt"),
                            )
                            .on_hover_text("0이면 크기 조건 없음")
                            .changed()
                        {
                            o.size_pt = (size > 0).then_some(size);
                        }
                    });
                    chrome::section(ui, "찾을 방향");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut o.direction, FindDirection::Down, "아래쪽");
                        ui.radio_value(&mut o.direction, FindDirection::Up, "위쪽");
                        ui.radio_value(&mut o.direction, FindDirection::All, "문서 전체");
                    });
                    ui.add_space(4.0);
                    let (status, color) = if self.find_state.status.is_empty() {
                        (
                            "본문과 표 셀에서 찾습니다. 바꿀 내용은 문자 그대로 넣습니다.",
                            Color32::GRAY,
                        )
                    } else if self.find_state.status_ok {
                        (self.find_state.status.as_str(), Color32::DARK_GRAY)
                    } else {
                        (self.find_state.status.as_str(), Color32::DARK_RED)
                    };
                    ui.add(egui::Label::new(RichText::new(status).size(12.5).color(color)).wrap());
                });
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    let button = |ui: &mut egui::Ui, label: &str, enabled: bool| {
                        ui.add_enabled(
                            enabled,
                            egui::Button::new(label).min_size(Vec2::new(92.0, 26.0)),
                        )
                        .on_disabled_hover_text("찾을 내용을 먼저 입력하세요.")
                        .clicked()
                    };
                    let has_query = !self.find.is_empty();
                    if replace {
                        next |= button(ui, "바꾸기", has_query);
                        if button(ui, "다음 찾기", has_query) {
                            let r = self.session.find(&self.find, self.find_state.options);
                            match self.result(r) {
                                Some(true) => self.found_status(),
                                Some(false) => self.not_found_status(),
                                None => {}
                            }
                            self.follow_cursor();
                        }
                        all = button(ui, "모두 바꾸기", has_query);
                    } else {
                        next |= button(ui, "다음 찾기", has_query);
                        if button(ui, "바꾸기...", true) {
                            self.find_state.replace = true;
                            self.find_state.focus = true;
                        }
                    }
                    close = button(ui, "닫기", true);
                });
            });
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                close = true;
            }
        });
        if next {
            self.find_step();
        }
        if all {
            let r = self.session.replace_all_with(
                &self.find,
                &self.replacement,
                self.find_state.options,
            );
            if let Some(count) = self.result(r) {
                self.find_state.status_ok = count > 0;
                self.find_state.status = if count == 0 {
                    "찾는 내용이 없습니다.".into()
                } else {
                    format!("{count}곳을 바꿨습니다. Ctrl+Z로 한 번에 되돌립니다.")
                };
                self.message = self.find_state.status.clone();
                self.sync_formatting();
            }
        }
        if close {
            self.show_find = false;
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("document-canvas")));
        }
    }
}
