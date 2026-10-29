//! Memo editor backed by the shared native field commands.
use super::*;
use serde_json::Value;

/// The memo pane docks on the right like Hangul's task panes; the page view narrows.
const MEMO_WIDTH: f32 = 400.0;

/// Disabled-control reasons shown as tooltips in the memo panel.
const NEW_MEMO_REASON: &str = "새 메모는 본문의 한 문단 안에서만 만들 수 있습니다. 표 밖 본문에 커서를 두거나 한 문단 안에서만 선택하세요.";
const STALE_REASON: &str = "문서가 바뀌었습니다. 새로 고침으로 메모를 다시 불러온 뒤 저장하세요.";
/// Memo input fields; document shortcuts pause only while one of them has focus.
const AUTHOR_FIELD: &str = "memo-author";
const TEXT_FIELD: &str = "memo-text";

pub(crate) struct State {
    pub open: bool,
    selected: Option<u32>,
    author: String,
    text: String,
    items: Vec<Value>,
    generation: Option<u64>,
    loaded_generation: Option<u64>,
    dirty: bool,
    notice: Option<String>,
    /// A new memo is being written; the input fields show only then or for a chosen memo.
    composing: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            selected: None,
            author: "사용자".into(),
            text: String::new(),
            items: Vec::new(),
            generation: None,
            loaded_generation: None,
            dirty: false,
            notice: None,
            composing: false,
        }
    }
}

impl State {
    fn load(&mut self, item: &Value) {
        self.selected = item["id"].as_u64().and_then(|id| u32::try_from(id).ok());
        self.author = item["author"].as_str().unwrap_or("사용자").to_owned();
        self.text = item["text"].as_str().unwrap_or("").to_owned();
        self.notice = None;
        self.loaded_generation = self.generation;
        self.dirty = false;
        self.composing = false;
    }
    /// Start writing a new memo (the input fields appear).
    fn new_memo(&mut self) {
        self.idle();
        self.composing = true;
    }
    /// No memo chosen and none being written: only the list and 새 메모 show.
    fn idle(&mut self) {
        self.selected = None;
        self.text.clear();
        self.notice = None;
        self.loaded_generation = self.generation;
        self.dirty = false;
        self.composing = false;
        if self.author.is_empty() {
            self.author = "사용자".into();
        }
    }
    fn editing(&self) -> bool {
        self.selected.is_some() || self.composing
    }
    fn refresh(&mut self, session: &Session) {
        if self.generation == Some(session.generation) {
            return;
        }
        self.items = session
            .memo_details()
            .as_array()
            .cloned()
            .unwrap_or_default();
        self.generation = Some(session.generation);
        if self.dirty {
            if self.loaded_generation != self.generation {
                self.notice =
                    Some("문서가 변경되었습니다. 메모를 다시 불러온 뒤 저장하세요.".into());
            }
        } else if let Some(id) = self.selected {
            if let Some(item) = self.items.iter().find(|item| item["id"] == id).cloned() {
                self.load(&item);
            } else {
                self.idle();
            }
        } else {
            self.loaded_generation = self.generation;
        }
    }
}

enum Action {
    Save,
    Delete,
    Goto,
}

impl HwpApp {
    pub(crate) fn comments_ext_enabled(&self, id: &str) -> Option<bool> {
        match id {
            "insert.memo" | "memo.create" => {
                Some(self.session.cursor.cell.is_none() && self.session.span_anchor.is_none())
            }
            "memo.list" | "view.memo" | "memo.edit" | "memo.delete" => Some(true),
            _ => None,
        }
    }
    pub(crate) fn comments_ext_dispatch(&mut self, id: &str, _ctx: &egui::Context) -> bool {
        if !matches!(
            id,
            "insert.memo" | "memo.create" | "memo.list" | "view.memo" | "memo.edit" | "memo.delete"
        ) {
            return false;
        }
        self.ext.comments.refresh(&self.session);
        self.ext.comments.open = true;
        if matches!(id, "insert.memo" | "memo.create") {
            self.ext.comments.new_memo();
        } else {
            let c = self.session.cursor;
            let item = self
                .ext
                .comments
                .items
                .iter()
                .find(|item| {
                    item["at"]["section"].as_u64() == Some(c.section as u64)
                        && item["at"]["paragraph"].as_u64() == Some(c.paragraph as u64)
                        && item["at"].get("cell").is_none()
                        && item["start"]
                            .as_u64()
                            .is_some_and(|start| start <= c.offset as u64)
                        && item["end"]
                            .as_u64()
                            .is_some_and(|end| end >= c.offset as u64)
                })
                .or_else(|| self.ext.comments.items.first())
                .cloned();
            if let Some(item) = item {
                self.ext.comments.load(&item);
            } else {
                self.ext.comments.idle();
            }
        }
        true
    }
    pub(crate) fn comments_ext_view(&mut self, name: &str) -> bool {
        match name {
            "memo" | "memo-list" => {
                let ctx = self.ctx.clone();
                self.comments_ext_dispatch("memo.list", &ctx)
            }
            "memo-create" => {
                let ctx = self.ctx.clone();
                self.comments_ext_dispatch("memo.create", &ctx)
            }
            _ => false,
        }
    }
    /// The docked pane leaves document shortcuts working; they pause only while a
    /// memo input field has keyboard focus.
    pub(crate) fn comments_ext_modal_open(&self) -> bool {
        self.ext.comments.open
            && self.ctx.memory(|m| {
                m.focused().is_some_and(|id| {
                    id == egui::Id::new(AUTHOR_FIELD) || id == egui::Id::new(TEXT_FIELD)
                })
            })
    }
    /// The memo pane, added before the page view so it narrows the view instead of
    /// covering the text.
    pub(crate) fn comments_panel(&mut self, ui: &mut egui::Ui) {
        if !self.ext.comments.open {
            return;
        }
        let mut state = std::mem::take(&mut self.ext.comments);
        state.refresh(&self.session);
        let can_create = self.session.cursor.cell.is_none() && self.session.span_anchor.is_none();
        let mut selected = state
            .selected
            .and_then(|id| state.items.iter().find(|item| item["id"] == id))
            .cloned();
        let open = state.open;
        let mut close_clicked = false;
        let mut action = None;
        egui::Panel::right("native-memo-editor")
            .default_size(MEMO_WIDTH)
            .size_range(320.0..=640.0)
            .resizable(true)
            .show(ui, |ui| {
                if chrome::title(ui, "메모") {
                    close_clicked = true;
                }
                let new_memo = |ui: &mut egui::Ui| {
                    ui.add_enabled(can_create, egui::Button::new("새 메모"))
                        .on_disabled_hover_text(NEW_MEMO_REASON)
                        .clicked()
                };
                if state.items.is_empty() {
                    if !state.composing {
                        super::ext_shell::note(
                            ui,
                            "이 문서에 메모가 없습니다. 본문에서 글을 선택하거나 커서를 둔 뒤 새 메모를 누르세요.",
                        );
                        if new_memo(ui) {
                            state.new_memo();
                        }
                    }
                } else {
                    ui.horizontal(|ui| {
                        ui.label(format!("메모 {}개", state.items.len()));
                        if !state.composing && new_memo(ui) {
                            state.new_memo();
                        }
                        if ui.button("새로 고침").clicked() {
                            state.generation = None;
                            state.refresh(&self.session);
                        }
                    });
                    egui::ScrollArea::vertical()
                        .id_salt("memo-list")
                        .max_height(180.0)
                        .show(ui, |ui| {
                            let mut choose = None;
                            for item in &state.items {
                                let Some(id) =
                                    item["id"].as_u64().and_then(|id| u32::try_from(id).ok())
                                else {
                                    continue;
                                };
                                let anchor: String = item["anchor"]
                                    .as_str()
                                    .unwrap_or("")
                                    .chars()
                                    .take(32)
                                    .collect();
                                let title: String = item["text"]
                                    .as_str()
                                    .unwrap_or("")
                                    .lines()
                                    .next()
                                    .unwrap_or("")
                                    .chars()
                                    .take(48)
                                    .collect();
                                let author = item["author"].as_str().unwrap_or("");
                                let label = if anchor.is_empty() {
                                    format!("{author}: {title}")
                                } else {
                                    format!("{author}: {title}  ({anchor})")
                                };
                                if ui
                                    .add(
                                        egui::Button::selectable(state.selected == Some(id), label)
                                            .truncate(),
                                    )
                                    .clicked()
                                {
                                    choose = Some(item.clone());
                                }
                            }
                            if let Some(item) = choose {
                                state.load(&item);
                            }
                        });
                }
                selected = state
                    .selected
                    .and_then(|id| state.items.iter().find(|item| item["id"] == id))
                    .cloned();
                if !state.editing() {
                    return;
                }
                let editable = selected
                    .as_ref()
                    .map_or(can_create, |item| item["editable"].as_bool() == Some(true));
                if !state.items.is_empty() {
                    ui.separator();
                }
                if let Some(item) = &selected {
                    let anchor = item["anchor"].as_str().unwrap_or("");
                    ui.horizontal_wrapped(|ui| {
                        ui.label(if anchor.is_empty() {
                            "기준 위치: 커서".to_owned()
                        } else {
                            format!("기준 글: {anchor}")
                        });
                        if ui
                            .add_enabled(
                                item["at"].get("cell").is_none(),
                                egui::Button::new("본문 위치"),
                            )
                            .on_disabled_hover_text("표 셀 안의 메모는 위치로 이동할 수 없습니다.")
                            .clicked()
                        {
                            action = Some(Action::Goto);
                        }
                    });
                } else {
                    let anchor = self.session.selected_text();
                    ui.label(if anchor.is_empty() {
                        "새 메모 기준: 현재 커서".to_owned()
                    } else {
                        format!("새 메모 기준: {anchor}")
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("작성자");
                    if ui
                        .add_enabled(
                            editable,
                            egui::TextEdit::singleline(&mut state.author)
                                .id(egui::Id::new(AUTHOR_FIELD))
                                .desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        state.dirty = true;
                    }
                });
                if ui
                    .add_enabled(
                        editable,
                        egui::TextEdit::multiline(&mut state.text)
                            .id(egui::Id::new(TEXT_FIELD))
                            .desired_width(f32::INFINITY)
                            .desired_rows(10)
                            .hint_text("메모 내용"),
                    )
                    .changed()
                {
                    state.dirty = true;
                }
                let readonly_reason = if selected.is_some() {
                    "표 셀 안의 메모는 읽기만 할 수 있습니다. 본문의 메모만 고칠 수 있습니다."
                } else {
                    NEW_MEMO_REASON
                };
                if !editable {
                    super::ext_shell::notice(ui, readonly_reason);
                }
                if let Some(notice) = &state.notice {
                    ui.label(notice);
                }
                let current = state.loaded_generation == Some(self.session.generation);
                if !current && ui.button("작성 내용 유지하고 새 메모로 연결").clicked()
                {
                    state.selected = None;
                    state.composing = true;
                    state.loaded_generation = Some(self.session.generation);
                    state.notice = None;
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            editable && current,
                            egui::Button::new(if state.selected.is_some() {
                                "변경 저장"
                            } else {
                                "저장"
                            }),
                        )
                        .on_disabled_hover_text(if !editable {
                            readonly_reason
                        } else {
                            STALE_REASON
                        })
                        .clicked()
                    {
                        action = Some(Action::Save);
                    }
                    if state.selected.is_none() {
                        // Discard the unsaved new memo and hide the input fields.
                        if ui.button("취소").clicked() {
                            state.idle();
                        }
                    } else if ui
                        .add_enabled(editable && current, egui::Button::new("메모 삭제"))
                        .on_disabled_hover_text(if !editable {
                            readonly_reason
                        } else {
                            STALE_REASON
                        })
                        .clicked()
                    {
                        action = Some(Action::Delete);
                    }
                });
                if state.selected.is_some() {
                    super::ext_shell::note(ui, "삭제 후 실행 취소로 복원할 수 있습니다.");
                }
            });
        state.open = open && !close_clicked;
        match action {
            Some(Action::Save) => {
                let result = if let Some(id) = state.selected {
                    self.session.run(
                        "memo.update",
                        json!({"id":id,"text":state.text,"author":state.author,"expected_generation":state.loaded_generation}),
                    )
                } else {
                    self.session.run(
                        "memo.create",
                        json!({"text":state.text,"author":state.author,"expected_generation":state.loaded_generation}),
                    )
                };
                if let Some(item) = self.result(result) {
                    state.generation = None;
                    state.refresh(&self.session);
                    state.load(&item);
                    state.notice = Some("메모를 저장했습니다.".into());
                    self.message = "메모를 저장했습니다.".into();
                }
            }
            Some(Action::Delete) => {
                if let Some(id) = state.selected {
                    let result = self.session.run(
                        "memo.delete",
                        json!({"id":id,"expected_generation":state.loaded_generation}),
                    );
                    if self.result(result).is_some() {
                        state.generation = None;
                        state.refresh(&self.session);
                        state.idle();
                        state.notice = Some("메모를 삭제했습니다. 본문은 유지됩니다.".into());
                    }
                }
            }
            Some(Action::Goto) => {
                if let Some(item) = selected {
                    let at = &item["at"];
                    if let (Some(section), Some(paragraph), Some(start), Some(end)) = (
                        at["section"].as_u64(),
                        at["paragraph"].as_u64(),
                        item["start"].as_u64(),
                        item["end"].as_u64(),
                    ) && at.get("cell").is_none()
                    {
                        let c = Cursor {
                            section: section as usize,
                            paragraph: paragraph as usize,
                            offset: start as usize,
                            cell: None,
                        };
                        let result = self.session.select(c, false).and_then(|_| {
                            self.session.select(
                                Cursor {
                                    offset: end as usize,
                                    ..c
                                },
                                true,
                            )
                        });
                        if self.result(result).is_some() {
                            self.page = self.session.page_of(c).unwrap_or(0) as u32;
                            self.scroll_to_caret = true;
                        }
                    }
                }
            }
            None => {}
        }
        self.ext.comments = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One frame of the memo pane beside a page view; the page view's width.
    fn page_width(app: &mut HwpApp) -> f32 {
        let ctx = app.ctx.clone();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1500.0, 900.0),
            )),
            ..Default::default()
        };
        let mut width = 0.0;
        let mut output = ctx.run_ui(input, |ui| {
            app.comments_panel(ui);
            egui::CentralPanel::default().show(ui, |ui| width = ui.max_rect().width());
        });
        output.textures_delta.clear();
        width
    }

    #[test]
    fn the_memo_pane_docks_on_the_right_and_narrows_the_page_view() -> anyhow::Result<()> {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        let full = page_width(&mut app);
        assert!(app.comments_ext_dispatch("view.memo", &app.ctx.clone()));
        let beside = page_width(&mut app);
        // The pane takes its own strip instead of floating over the text.
        assert!(full - beside >= 320.0, "{full} → {beside}");
        assert!(full - beside <= 660.0, "{full} → {beside}");
        // No floating window is left over the page.
        let window = egui::Id::new("native-memo-editor");
        assert!(app.ctx.memory(|m| {
            m.areas()
                .visible_layer_ids()
                .iter()
                .all(|layer| layer.id != window)
        }));
        app.ext.comments.open = false;
        assert_eq!(page_width(&mut app), full);
        Ok(())
    }
    #[test]
    fn memo_inputs_appear_only_after_new_memo_and_shortcuts_pause_only_while_typing()
    -> anyhow::Result<()> {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        app.session.execute(Command::Insert("메모 대상".into()))?;
        let text_field = egui::Id::new(TEXT_FIELD);
        let shown = |app: &HwpApp| egui::TextEdit::load_state(&app.ctx, text_field).is_some();
        // No memos: the pane opens without input fields and keeps shortcuts working.
        assert!(app.comments_ext_dispatch("view.memo", &app.ctx.clone()));
        page_width(&mut app);
        assert!(!app.ext.comments.editing());
        assert!(!shown(&app), "no input fields before 새 메모");
        assert!(!app.comments_ext_modal_open());
        // 새 메모 (same as memo.create) shows them; focus in the text pauses shortcuts.
        assert!(app.comments_ext_dispatch("memo.create", &app.ctx.clone()));
        page_width(&mut app);
        assert!(shown(&app));
        assert!(
            !app.comments_ext_modal_open(),
            "open pane alone does not block keys"
        );
        app.ctx.memory_mut(|m| m.request_focus(text_field));
        assert!(app.comments_ext_modal_open());
        app.ctx.memory_mut(|m| m.surrender_focus(text_field));
        // Saving lists the memo; 취소-equivalent idle hides the fields again.
        app.ext.comments.text = "검토 필요".into();
        let generation = app.ext.comments.loaded_generation;
        app.session.run(
            "memo.create",
            json!({"text":"검토 필요","expected_generation":generation}),
        )?;
        app.ext.comments.idle();
        page_width(&mut app);
        assert_eq!(app.ext.comments.items.len(), 1);
        assert!(!app.ext.comments.editing());
        Ok(())
    }
    #[test]
    fn list_refresh_keeps_a_dirty_draft_and_its_original_generation() -> anyhow::Result<()> {
        let mut session = Session::new()?;
        session.execute(Command::Insert("메모 대상".into()))?;
        let original = session.run("memo.create", json!({"text":"원래 내용"}))?;
        let mut state = State::default();
        state.refresh(&session);
        state.load(&original);
        let loaded = state.loaded_generation;
        state.text = "아직 저장하지 않은 내용".into();
        state.dirty = true;
        session.run(
            "memo.update",
            json!({"id":original["id"],"text":"다른 편집"}),
        )?;
        state.refresh(&session);
        assert_eq!(state.text, "아직 저장하지 않은 내용");
        assert_eq!(state.loaded_generation, loaded);
        assert_ne!(state.loaded_generation, state.generation);
        assert!(state.notice.is_some());
        let current = state.items[0].clone();
        state.load(&current);
        assert_eq!(state.text, "다른 편집");
        assert_eq!(state.loaded_generation, state.generation);
        assert!(!state.dirty);
        Ok(())
    }
}
