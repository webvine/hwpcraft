//! The panel's screen, laid out like the Claude and Codex apps: a header with
//! the engine choice, the conversation (your messages as right-aligned
//! bubbles, the AI's answers as full-width text with its commands folded into
//! one row), and a composer card at the bottom with the context chip, the input
//! box and the send / stop button. Everything wraps or truncates to the panel's
//! width (the shell lets it be 260–560 px).
use super::*;

const USER_FILL: Color32 = Color32::from_rgb(237, 241, 248);
const SUBTLE: Color32 = Color32::from_rgb(108, 114, 124);
const STEP_FILL: Color32 = Color32::from_rgb(246, 247, 249);
const CHIP_FILL: Color32 = Color32::from_rgb(232, 240, 255);
const OK: Color32 = Color32::from_rgb(46, 125, 50);
const ERROR: Color32 = Color32::from_rgb(198, 40, 40);
const ERROR_FILL: Color32 = Color32::from_rgb(253, 236, 236);

/// Requests the empty panel offers; a click puts one in the input box.
const EXAMPLES: [&str; 3] = [
    "선택한 부분을 더 공손한 문장으로 다듬어 줘",
    "이 문단 내용을 3열 표로 정리해 줘",
    "문서 끝에 전체 내용을 세 줄로 요약해 줘",
];

fn wrapped(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(egui::Label::new(text).wrap())
}

/// A rounded box as wide as the panel.
fn card(fill: Color32, stroke: Option<Color32>) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .stroke(stroke.map_or(Stroke::NONE, |c| Stroke::new(1.0, c)))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(8, 5))
}

impl HwpApp {
    /// The panel's contents.
    pub(crate) fn ai_panel_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.ai_detect(&ctx);
        self.ai_history_follow();
        let busy = self.ai_busy();
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
        self.ai_header(ui, busy);
        ui.separator();
        egui::Panel::bottom("ai-panel-composer")
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new())
            .show(ui, |ui| self.ai_composer(ui, &ctx, busy));
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            // An empty conversation (the welcome text) reads from the top.
            .stick_to_bottom(!self.ext.ai.messages.is_empty())
            .show(ui, |ui| {
                // Keep every row inside the panel; the scroll bar takes the rest.
                ui.set_max_width(ui.available_width());
                self.ai_messages(ui);
            });
    }

    fn ai_header(&mut self, ui: &mut egui::Ui, busy: bool) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("AI 편집").strong().size(14.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(egui::Button::new("✕").frame(false))
                    .on_hover_text("패널 닫기 (실행 중이면 계속 실행됩니다)")
                    .clicked()
                {
                    self.ext.ai.close_requested = true;
                }
                if ui
                    .add_enabled(!busy, egui::Button::new("새 대화").small())
                    .on_hover_text("대화를 비우고 처음부터 시작합니다")
                    .clicked()
                {
                    self.ai_new_conversation();
                }
                let current = self.ext.ai.engine;
                ui.add_enabled_ui(!busy, |ui| {
                    egui::ComboBox::from_id_salt("ai-engine")
                        .selected_text(current.name())
                        .width(92.0)
                        .show_ui(ui, |ui| {
                            for (engine, cli) in self.ext.ai.clis.clone().unwrap_or_default() {
                                let r = ui.add_enabled(
                                    cli.unusable.is_none(),
                                    egui::Button::selectable(current == engine, engine.name()),
                                );
                                let r = match &cli.unusable {
                                    Some(why) => r.on_disabled_hover_text(why),
                                    None => r,
                                };
                                if r.clicked() && current != engine {
                                    self.ext.ai.engine = engine;
                                    // Another engine cannot continue this
                                    // conversation: start a new one rather
                                    // than show turns it does not know.
                                    if self.ext.ai.messages.is_empty() {
                                        self.ext.ai.conversation = None;
                                    } else {
                                        self.ai_new_conversation();
                                    }
                                }
                            }
                        });
                });
            });
        });
        self.ai_model_row(ui, busy);
        let (text, color) = self.ai_status();
        wrapped(ui, RichText::new(text).small().color(color));
        self.ai_history(ui, busy);
    }

    /// This document's conversation history: one folded row, opened into the
    /// list with the switches and the delete buttons.
    fn ai_history(&mut self, ui: &mut egui::Ui, busy: bool) {
        let h = &self.ext.ai.hist;
        let label = format!(
            "{} 이 문서의 대화 기록 {}개{}",
            if h.open { "▾" } else { "▸" },
            h.found.len(),
            if h.on { "" } else { " · 기록 꺼짐" }
        );
        let r = ui
            .add(
                egui::Label::new(RichText::new(label).small().color(SUBTLE))
                    .truncate()
                    .sense(egui::Sense::click()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("이 문서에서 나눈 대화를 불러오거나 지웁니다");
        if r.clicked() {
            self.ext.ai.hist.open = !self.ext.ai.hist.open;
            self.ext.ai.hist.confirm = None;
            if self.ext.ai.hist.open {
                self.ai_history_scan();
            }
        }
        if let Some(n) = &self.ext.ai.hist.notice {
            wrapped(ui, RichText::new(n).small().color(chrome::Tokens::ACCENT));
        }
        if !self.ext.ai.hist.open {
            return;
        }
        let mut act: Option<HistoryAct> = None;
        let found = self.ext.ai.hist.found.clone();
        let confirm = self.ext.ai.hist.confirm.clone();
        let current = self.ext.ai.hist.current.as_ref().map(|c| c.0.clone());
        // The list may take at most a third of the panel; the conversation
        // keeps the rest.
        let max_height = (ui.available_height() * 0.35).clamp(120.0, 280.0);
        let list = egui::ScrollArea::vertical()
            .id_salt("ai-history-list")
            .max_height(max_height)
            .auto_shrink([false, true]);
        list.show(ui, |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                ui.set_max_width(ui.available_width());
                if found.is_empty() {
                    wrapped(
                        ui,
                        RichText::new("아직 이 문서의 대화 기록이 없습니다.")
                            .small()
                            .color(SUBTLE),
                    );
                }
                for (c, rel) in &found {
                    card(Color32::WHITE, Some(chrome::Tokens::BORDER)).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.add(
                            egui::Label::new(RichText::new(&c.title).small().strong()).truncate(),
                        );
                        let note = match rel {
                            history::Relation::Same => "",
                            history::Relation::Changed => " · 그 뒤 문서가 바뀜",
                            history::Relation::Moved => " · 다른 위치의 같은 문서일 수 있음",
                        };
                        wrapped(
                            ui,
                            RichText::new(format!(
                                "{} · 질문 {}개 · {}{note}",
                                c.engine,
                                c.turns,
                                ago(c.updated)
                            ))
                            .small()
                            .color(SUBTLE),
                        );
                        ui.horizontal_wrapped(|ui| {
                            if current.as_deref() == Some(c.id.as_str()) {
                                ui.label(RichText::new("지금 대화").small().color(OK));
                            } else if ui.small_button("불러오기").clicked() {
                                act = Some(HistoryAct::Open(c.id.clone()));
                            }
                            if ui
                                .small_button("내보내기")
                                .on_hover_text("Markdown 파일로 저장")
                                .clicked()
                            {
                                act = Some(HistoryAct::Export(c.id.clone()));
                            }
                            let doomed = history::Doomed::One(c.id.clone());
                            delete_button(ui, "지우기", &doomed, confirm.as_ref(), &mut act);
                        });
                    });
                }
            })
        });
        // The switches and the delete-all buttons, folded away by default.
        let fold = if self.ext.ai.history_settings {
            "▾ 기록 설정"
        } else {
            "▸ 기록 설정"
        };
        if ui
            .add(
                egui::Label::new(RichText::new(fold).small().color(SUBTLE))
                    .sense(egui::Sense::click()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            self.ext.ai.history_settings = !self.ext.ai.history_settings;
        }
        if self.ext.ai.history_settings {
            ui.add_enabled_ui(!busy, |ui| {
            let (mut on, mut track) = (self.ext.ai.hist.on, self.ext.ai.hist.track);
            if ui
                .checkbox(&mut on, RichText::new("대화 기록 남기기").small())
                .on_hover_text("질문과 답, 바뀐 곳을 이 컴퓨터의 설정 폴더에 남깁니다")
                .changed()
            {
                act = Some(HistoryAct::Set(Some(on), None));
            }
            if ui
                .checkbox(&mut track, RichText::new("글만 고친 질문은 변경 추적으로 남기기").small())
                .on_hover_text(
                    "AI가 글만 바꾼 질문을 문서의 변경 추적(작성자 'AI · 엔진 이름')으로 다시 적용해, \
                     검토 탭에서 하나씩 수락·거부할 수 있게 합니다. 서식·표·개체를 바꾼 질문과 이미 \
                     변경 추적이 있는 문서는 그대로 둡니다.",
                )
                .changed()
            {
                act = Some(HistoryAct::Set(None, Some(track)));
            }
            let mut copies = !self.ext.ai.hist.keep_cli_copies;
            if ui
                .checkbox(&mut copies, RichText::new("지울 때 Claude Code·Codex 쪽 대화도 지우기").small())
                .changed()
            {
                self.ext.ai.hist.keep_cli_copies = !copies;
            }
            ui.horizontal_wrapped(|ui| {
                delete_button(ui, "이 문서 기록 지우기", &history::Doomed::Document, confirm.as_ref(), &mut act);
                delete_button(ui, "모든 기록 지우기", &history::Doomed::All, confirm.as_ref(), &mut act);
            });
            wrapped(
                ui,
                RichText::new(
                    "기록은 이 컴퓨터의 설정 폴더(ai-history)에 이 사용자만 읽게 저장되고, 문서 파일에는 들어가지 않습니다.",
                )
                .small()
                .color(SUBTLE),
            );
            });
        }
        match act {
            Some(HistoryAct::Open(id)) => {
                self.ai_history_open(&id);
                // Straight into the loaded conversation.
                self.ext.ai.hist.open = false;
            }
            Some(HistoryAct::Export(id)) => self.ai_history_export(&id),
            Some(HistoryAct::Confirm(d)) => self.ext.ai.hist.confirm = Some(d),
            Some(HistoryAct::Cancel) => self.ext.ai.hist.confirm = None,
            Some(HistoryAct::Delete(d)) => self.ai_history_delete(&d),
            Some(HistoryAct::Set(on, track)) => self.ai_history_set(on, track),
            None => {}
        }
    }

    /// The model and reasoning-level menus for the chosen engine.
    fn ai_model_row(&mut self, ui: &mut egui::Ui, busy: bool) {
        let engine = self.ext.ai.engine;
        let slot = engine.slot();
        let list = self.ext.ai.models[slot].clone();
        let (model, effort) = self.ai_choice(engine);
        let Some(model) = model else {
            return;
        };
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| {
                let half = ((ui.available_width() - 70.0) / 2.0).max(60.0);
                ui.label(RichText::new("모델").small().color(SUBTLE));
                egui::ComboBox::from_id_salt(("ai-model", slot))
                    .selected_text(RichText::new(&model.name).small())
                    .width(half)
                    .show_ui(ui, |ui| {
                        for (i, m) in list.iter().enumerate() {
                            let chosen = self.ext.ai.model_pick[slot] == i;
                            let r = ui.selectable_label(chosen, &m.name);
                            let r = if m.note.is_empty() {
                                r
                            } else {
                                r.on_hover_text(&m.note)
                            };
                            if r.clicked() {
                                self.ext.ai.model_pick[slot] = i;
                            }
                        }
                    })
                    .response
                    .on_hover_text(if model.note.is_empty() {
                        "AI가 쓸 모델"
                    } else {
                        &model.note
                    });
                ui.label(RichText::new("추론").small().color(SUBTLE));
                let shown = match (&effort, &model.default_effort) {
                    (Some(e), _) => models::effort_label(e),
                    (None, Some(d)) => format!("기본 ({d})"),
                    (None, None) => "기본".to_owned(),
                };
                ui.add_enabled_ui(!model.efforts.is_empty(), |ui| {
                    egui::ComboBox::from_id_salt(("ai-effort", slot))
                        .selected_text(RichText::new(shown).small())
                        .width(half)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(effort.is_none(), "기본 (모델 기본값)")
                                .clicked()
                            {
                                self.ext.ai.effort_pick[slot] = None;
                            }
                            for e in &model.efforts {
                                if ui
                                    .selectable_label(
                                        effort.as_deref() == Some(e),
                                        models::effort_label(e),
                                    )
                                    .clicked()
                                {
                                    self.ext.ai.effort_pick[slot] = Some(e.clone());
                                }
                            }
                        })
                        .response
                        .on_hover_text(
                            "빠르게는 답이 빠르고, 깊게·최대는 오래 생각해 복잡한 편집에 맞습니다.",
                        );
                });
            });
        });
    }

    /// The line under the title: who works on what, or why it cannot.
    fn ai_status(&self) -> (String, Color32) {
        let engine = self.ext.ai.engine;
        if self.ext.ai.clis.is_none() {
            return ("Claude Code·Codex를 확인하는 중입니다…".to_owned(), SUBTLE);
        }
        if self.ext.ai.sample_busy.is_some() {
            return (
                format!("{}가 작업 중입니다", engine.name()),
                chrome::Tokens::ACCENT,
            );
        }
        if let Some(run) = &self.ext.ai.run {
            return (
                format!(
                    "{}가 작업 중입니다 · {}초",
                    run.engine.name(),
                    run.running.started.elapsed().as_secs()
                ),
                chrome::Tokens::ACCENT,
            );
        }
        let unusable = self
            .ext
            .ai
            .clis
            .as_ref()
            .and_then(|c| c.iter().find(|(e, _)| *e == engine))
            .and_then(|(_, c)| c.unusable.clone());
        match unusable {
            Some(why) => (why, ERROR),
            // Idle: ready, not working (the running line is blue).
            None => (
                format!("● {}로 이 문서를 편집할 수 있습니다", engine.name()),
                SUBTLE,
            ),
        }
    }

    fn ai_messages(&mut self, ui: &mut egui::Ui) {
        if self.ext.ai.messages.is_empty() {
            self.ai_empty(ui);
            return;
        }
        let running = self
            .ext
            .ai
            .run
            .as_ref()
            .map(|r| r.message)
            .or(self.ext.ai.sample_busy);
        let busy = self.ai_busy();
        let mut act = None;
        ui.add_space(4.0);
        for (i, m) in self.ext.ai.messages.iter_mut().enumerate() {
            if m.user {
                user_message(ui, m);
            } else {
                ui.vertical(|ui| {
                    ai_message(ui, m, running == Some(i));
                    if running != Some(i) {
                        changes_row(ui, m, i, busy, &mut act);
                    }
                });
            }
            ui.add_space(10.0);
        }
        match act {
            Some(ChangeAct::Jump(i, k)) => self.ai_history_jump(i, k),
            Some(ChangeAct::Revert(i)) => self.ai_history_revert(i),
            None => {}
        }
    }

    /// The empty conversation: what the panel does, with example requests.
    fn ai_empty(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        wrapped(
            ui,
            RichText::new("무엇을 고쳐 드릴까요?").strong().size(16.0),
        );
        wrapped(
            ui,
            RichText::new(crate::ribbon::platform_keys(
                "선택한 곳이나 커서가 있는 문단이 함께 전달되어 \"이 부분\"이 그곳을 뜻합니다. \
                 AI의 편집은 바로 적용되고, 질문 하나를 Ctrl+Z 한 번으로 되돌립니다.",
            ))
            .small()
            .color(SUBTLE),
        );
        ui.add_space(6.0);
        for example in EXAMPLES {
            let r = card(Color32::WHITE, Some(chrome::Tokens::BORDER))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    wrapped(ui, RichText::new(example).small());
                })
                .response
                .interact(egui::Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("입력란에 넣기");
            if r.clicked() {
                self.ext.ai.input = example.to_owned();
                ui.ctx()
                    .memory_mut(|m| m.request_focus(egui::Id::new("ai-panel-input-box")));
            }
        }
    }

    /// The composer card: context chip, input box, hint and send / stop.
    fn ai_composer(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, busy: bool) {
        ui.add_space(6.0);
        egui::Frame::new()
            .fill(Color32::WHITE)
            .stroke(Stroke::new(1.0, chrome::Tokens::BORDER))
            .corner_radius(12)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                self.ai_context_chip(ui);
                let edit = egui::TextEdit::multiline(&mut self.ext.ai.input)
                    .id(egui::Id::new("ai-panel-input-box"))
                    .hint_text("AI에게 요청하기…")
                    .frame(egui::Frame::new())
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .return_key(egui::KeyboardShortcut::new(
                        egui::Modifiers::SHIFT,
                        Key::Enter,
                    ));
                let r = ui.add_enabled(!busy, edit);
                let enter =
                    r.has_focus() && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
                // An engine that cannot run is said here, not after sending.
                let blocked = self.ai_unusable();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if busy {
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("중지").color(Color32::WHITE))
                                        .fill(ERROR)
                                        .corner_radius(8)
                                        .min_size(Vec2::new(56.0, 26.0)),
                                )
                                .on_hover_text(crate::ribbon::platform_keys(
                                    "AI를 멈춥니다. 그때까지의 편집은 남고, Ctrl+Z 한 번으로 되돌립니다.",
                                ))
                                .clicked()
                            {
                                self.ai_stop();
                            }
                        } else {
                            let send = ui
                                .add_enabled(
                                    !self.ext.ai.input.trim().is_empty() && blocked.is_none(),
                                    egui::Button::new(
                                        RichText::new("보내기").color(Color32::WHITE),
                                    )
                                    .fill(chrome::Tokens::ACCENT)
                                    .corner_radius(8)
                                    .min_size(Vec2::new(56.0, 26.0)),
                                )
                                .clicked();
                            if (send || enter) && blocked.is_none() {
                                self.ai_send(ctx);
                                r.request_focus();
                            }
                        }
                        match &blocked {
                            Some(why) => {
                                ui.add(
                                    egui::Label::new(RichText::new(why).small().color(ERROR))
                                        .truncate(),
                                )
                                .on_hover_text(why);
                            }
                            None => {
                                let hint = if busy {
                                    "답을 기다리는 중…"
                                } else {
                                    "Enter 보내기 · Shift+Enter 줄바꿈"
                                };
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(hint)
                                            .small()
                                            .color(SUBTLE),
                                    )
                                    .truncate(),
                                );
                            }
                        }
                    });
                });
            });
        ui.add_space(6.0);
    }

    /// What "this part" will mean, with a ✕ to send the question without it.
    fn ai_context_chip(&mut self, ui: &mut egui::Ui) {
        let context = (!self.ext.ai.skip_context)
            .then(|| self.ai_chip_text())
            .flatten();
        if context.is_none() && !self.ext.ai.skip_context {
            return;
        }
        let fill = if context.is_some() {
            CHIP_FILL
        } else {
            STEP_FILL
        };
        egui::Frame::new()
            .fill(fill)
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        match &context {
                            Some(c) => {
                                if ui
                                    .add(egui::Button::new("✕").frame(false).small())
                                    .on_hover_text("이번 질문은 영역 없이 보냅니다")
                                    .clicked()
                                {
                                    self.ext.ai.skip_context = true;
                                }
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(c.as_str())
                                                    .small()
                                                    .color(chrome::Tokens::ACCENT),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(
                                            format!("{c}\n보낼 때 이 영역이 함께 전달됩니다."),
                                        );
                                    },
                                );
                            }
                            None => {
                                if ui
                                    .add(egui::Button::new("되돌리기").frame(false).small())
                                    .clicked()
                                {
                                    self.ext.ai.skip_context = false;
                                }
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new("영역 없이 보냅니다")
                                                    .small()
                                                    .color(SUBTLE),
                                            )
                                            .truncate(),
                                        );
                                    },
                                );
                            }
                        }
                    });
                });
            });
    }
}

/// What a click in the history list asks for.
enum HistoryAct {
    Open(String),
    Export(String),
    Confirm(history::Doomed),
    Cancel,
    Delete(history::Doomed),
    Set(Option<bool>, Option<bool>),
}

/// What a click on an AI bubble's changed places asks for.
enum ChangeAct {
    Jump(usize, usize),
    Revert(usize),
}

/// A delete button that asks once more: 정말 지우기 / 취소.
fn delete_button(
    ui: &mut egui::Ui,
    label: &str,
    doomed: &history::Doomed,
    confirm: Option<&history::Doomed>,
    act: &mut Option<HistoryAct>,
) {
    if confirm == Some(doomed) {
        if ui
            .small_button(RichText::new("정말 지우기").color(ERROR))
            .clicked()
        {
            *act = Some(HistoryAct::Delete(doomed.clone()));
        }
        if ui.small_button("취소").clicked() {
            *act = Some(HistoryAct::Cancel);
        }
    } else if ui.small_button(label).clicked() {
        *act = Some(HistoryAct::Confirm(doomed.clone()));
    }
}

/// "3분 전" for a Unix time.
fn ago(at: u64) -> String {
    let s = history::now().saturating_sub(at);
    match s {
        0..60 => "방금".into(),
        60..3600 => format!("{}분 전", s / 60),
        3600..86400 => format!("{}시간 전", s / 3600),
        _ => format!("{}일 전", s / 86400),
    }
}

/// Changed places shown per answer; the rest are counted.
const SHOWN_CHANGES: usize = 6;

/// Where an answer changed the document (each line moves the caret there) and
/// the button that takes the question back.
fn changes_row(ui: &mut egui::Ui, m: &Message, i: usize, busy: bool, act: &mut Option<ChangeAct>) {
    let Some(link) = &m.link else {
        return;
    };
    if link.changes.is_empty() && link.track_note.is_none() {
        return;
    }
    card(STEP_FILL, None).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        if !link.changes.is_empty() {
            ui.add(
                egui::Label::new(
                    RichText::new(format!("바뀐 곳 {}곳", link.changes.len()))
                        .small()
                        .color(SUBTLE),
                )
                .truncate(),
            );
        }
        for (k, c) in link.changes.iter().enumerate().take(SHOWN_CHANGES) {
            let (verb, text) = match c.kind {
                changes::Kind::Modify => ("고침", &c.after),
                changes::Kind::Insert => ("넣음", &c.after),
                changes::Kind::Delete => ("지움", &c.before),
            };
            let text = text.as_deref().map(str::trim).unwrap_or_default();
            let text = if text.is_empty() {
                "(빈 문단)"
            } else {
                text
            };
            let r = ui
                .add(
                    egui::Label::new(
                        RichText::new(format!("{verb} · {text}"))
                            .small()
                            .color(chrome::Tokens::ACCENT),
                    )
                    .truncate()
                    .sense(egui::Sense::click()),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(if busy {
                    "AI가 편집하는 동안에는 캐럿을 옮기지 않습니다"
                } else {
                    "문서에서 이 곳으로 이동"
                });
            // Moving the caret while the AI edits would move it under the AI.
            if r.clicked() && !busy {
                *act = Some(ChangeAct::Jump(i, k));
            }
        }
        if link.changes.len() > SHOWN_CHANGES {
            ui.label(
                RichText::new(format!("외 {}곳", link.changes.len() - SHOWN_CHANGES))
                    .small()
                    .color(SUBTLE),
            );
        }
        ui.horizontal_wrapped(|ui| {
            if link.reverted {
                ui.label(RichText::new("되돌렸습니다").small().color(SUBTLE));
            } else if !link.changes.is_empty()
                && ui
                    .add_enabled(!busy, egui::Button::new(RichText::new("되돌리기").small()))
                    .on_hover_text(
                        "이 질문의 편집만 되돌립니다. 그 뒤 손으로 고친 곳은 남기고, \
                         바뀐 글을 찾지 못하면 아무것도 바꾸지 않습니다.",
                    )
                    .clicked()
            {
                *act = Some(ChangeAct::Revert(i));
            }
            if let Some(author) = &link.tracked {
                ui.label(RichText::new("변경 추적으로 남김").small().color(OK))
                    .on_hover_text(format!("작성자 '{author}' · 검토 탭에서 수락·거부합니다"));
            }
        });
        if let Some(note) = &link.track_note {
            wrapped(
                ui,
                RichText::new(format!("변경 추적으로 남기지 않음: {note}"))
                    .small()
                    .color(SUBTLE),
            );
        }
    });
}

/// Your message: a right-aligned bubble that wraps at 85% of the panel, with
/// the region it was sent with under it.
fn user_message(ui: &mut egui::Ui, m: &Message) {
    let max = ui.available_width() * 0.85;
    ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
        egui::Frame::new()
            .fill(USER_FILL)
            .corner_radius(12)
            .inner_margin(egui::Margin::symmetric(10, 7))
            .show(ui, |ui| {
                ui.set_max_width(max);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    wrapped(ui, RichText::new(&m.text));
                });
            });
        if let Some(c) = &m.context {
            ui.scope(|ui| {
                ui.set_max_width(max);
                wrapped(ui, RichText::new(c).small().color(SUBTLE));
            });
        }
    });
}

/// The AI's answer: its commands folded into one row, the text, then notes.
fn ai_message(ui: &mut egui::Ui, m: &mut Message, running: bool) {
    if let Some(t) = &m.turn {
        if running {
            card(STEP_FILL, None).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(12.0));
                    let now = t.running().map_or_else(
                        || "생각하는 중".to_owned(),
                        |label| format!("{label} 실행 중"),
                    );
                    let done = t.tools.iter().filter(|x| x.2).count();
                    let text = if done > 0 {
                        format!("{now} · {done}개 끝남")
                    } else {
                        now
                    };
                    ui.add(egui::Label::new(RichText::new(text).small().color(SUBTLE)).truncate());
                });
            });
        } else if !t.tools.is_empty() {
            steps(ui, m.expanded, t).then(|| m.expanded = !m.expanded);
        }
        let answer = t
            .answer
            .clone()
            .or_else(|| running.then(|| t.texts.last().cloned()).flatten());
        if let Some(a) = answer {
            markdown::show(ui, &a);
        }
    }
    if m.stopped {
        wrapped(
            ui,
            RichText::new("중지했습니다. 그때까지의 편집은 남아 있습니다.")
                .small()
                .color(SUBTLE),
        );
    }
    if let Some(e) = &m.error {
        card(ERROR_FILL, None).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            wrapped(ui, RichText::new(e).small().color(ERROR));
        });
    }
    // The changed-places card has its own 되돌리기; the note is for answers
    // without one (formatting, tables, objects).
    let card = m.link.as_ref().is_some_and(|l| !l.changes.is_empty());
    if m.changed && !running && !card {
        wrapped(
            ui,
            RichText::new(crate::ribbon::platform_keys(
                "문서를 고쳤습니다 · Ctrl+Z 한 번으로 되돌릴 수 있습니다",
            ))
            .small()
            .color(SUBTLE),
        );
    }
}

/// The folded commands row; returns true when it was clicked (open / close).
fn steps(ui: &mut egui::Ui, open: bool, t: &Turn) -> bool {
    let failed = t.failed();
    let summary = format!(
        "{}  명령 {}개 실행{}",
        if open { "▾" } else { "▸" },
        t.tools.len(),
        if failed > 0 {
            format!(" · {failed}개 실패")
        } else {
            String::new()
        }
    );
    let r = card(STEP_FILL, None)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.add(
                egui::Label::new(RichText::new(summary).small().color(if failed > 0 {
                    ERROR
                } else {
                    SUBTLE
                }))
                .truncate(),
            );
            if open {
                for (_, label, _, error) in &t.tools {
                    let (mark, color) = match error {
                        None => ("✓", OK),
                        Some(_) => ("✗", ERROR),
                    };
                    let line = ui.add(
                        egui::Label::new(
                            RichText::new(format!("{mark} {label}"))
                                .small()
                                .monospace()
                                .color(color),
                        )
                        .truncate(),
                    );
                    if let Some(e) = error {
                        line.on_hover_text(e);
                    }
                }
            }
        })
        .response
        .interact(egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if open { "접기" } else { "펼치기" });
    r.clicked()
}
