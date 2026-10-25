//! Owner: core area. Core editing tools on the ribbon: 정렬, 조판 부호 지우기, 이동(Alt+G).
use super::*;
use hwpcraft_engine::{MarkKind, SortKey, SortOptions};

/// Hangul's 이동 (찾아가기) targets HwpCraft can reach.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GotoKind {
    Page,
    Section,
    Paragraph,
    Bookmark,
}

pub(crate) struct GotoState {
    kind: GotoKind,
    value: String,
    bookmarks: Vec<String>,
    bookmark: usize,
}

#[derive(Default)]
pub(crate) struct State {
    sort: Option<SortOptions>,
    marks: Option<Vec<(MarkKind, usize, bool)>>,
    goto: Option<GotoState>,
}

impl HwpApp {
    pub(crate) fn core_ext_enabled(&self, id: &str) -> Option<bool> {
        match id {
            // Like Hangul, 정렬 needs a block of paragraphs.
            "edit.sort" => {
                Some(self.session.cursor.cell.is_none() && self.session.span().is_some())
            }
            "edit.delete_ctrl_marks" => Some(self.session.cursor.cell.is_none()),
            "edit.goto" => Some(true),
            _ => None,
        }
    }
    pub(crate) fn core_ext_dispatch(&mut self, id: &str) -> bool {
        match id {
            "edit.sort" => self.ext.core.sort = Some(SortOptions::default()),
            "edit.delete_ctrl_marks" => {
                let counts = self.session.ctrl_mark_counts();
                if let Some(counts) = self.result(counts) {
                    self.ext.core.marks =
                        Some(counts.into_iter().map(|(k, n)| (k, n, n > 0)).collect());
                }
            }
            "edit.goto" => {
                let bookmarks: Vec<String> = self
                    .session
                    .bookmarks()
                    .ok()
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|b| b["name"].as_str().map(str::to_owned))
                    .collect();
                let page = self.session.page_of(self.session.cursor).unwrap_or(0) + 1;
                self.ext.core.goto = Some(GotoState {
                    kind: GotoKind::Page,
                    value: page.to_string(),
                    bookmarks,
                    bookmark: 0,
                });
            }
            _ => return false,
        }
        true
    }
    pub(crate) fn core_ext_modal_open(&self) -> bool {
        self.ext.core.sort.is_some()
            || self.ext.core.marks.is_some()
            || self.ext.core.goto.is_some()
    }
    pub(crate) fn core_ext_view(&mut self, name: &str) -> bool {
        match name {
            "sort" => self.core_ext_dispatch("edit.sort"),
            "ctrl-marks" => self.core_ext_dispatch("edit.delete_ctrl_marks"),
            "goto" => self.core_ext_dispatch("edit.goto"),
            _ => false,
        }
    }
    pub(crate) fn core_ext_dialogs(&mut self, ctx: &egui::Context) {
        self.sort_dialog(ctx);
        self.marks_dialog(ctx);
        self.goto_dialog(ctx);
    }
    /// Hangul 이동 (Alt+G): jump to a page, section, paragraph or bookmark.
    fn goto_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut g) = self.ext.core.goto.take() else {
            return;
        };
        let pages = self.session.core.page_count();
        let sections = self.session.core.document().sections.len();
        let paragraphs = self
            .session
            .core
            .document()
            .sections
            .get(self.session.cursor.section)
            .map(|s| s.paragraphs.len())
            .unwrap_or(0);
        let (mut apply, mut cancel) = (false, false);
        chrome::modal("goto-dialog").show(ctx, |ui| {
            ui.set_width(380.0);
            cancel = chrome::title(ui, "이동");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(270.0);
                    chrome::section(ui, "이동할 곳");
                    let before = g.kind;
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut g.kind, GotoKind::Page, format!("쪽 (1–{pages})"));
                        ui.radio_value(
                            &mut g.kind,
                            GotoKind::Section,
                            format!("구역 (1–{sections})"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut g.kind,
                            GotoKind::Paragraph,
                            format!("문단 (1–{paragraphs})"),
                        );
                        ui.add_enabled_ui(!g.bookmarks.is_empty(), |ui| {
                            ui.radio_value(&mut g.kind, GotoKind::Bookmark, "책갈피");
                        });
                    });
                    if g.kind != before {
                        g.value = "1".into();
                    }
                    if g.kind == GotoKind::Bookmark {
                        egui::ComboBox::from_id_salt("goto-bookmark")
                            .selected_text(g.bookmarks.get(g.bookmark).cloned().unwrap_or_default())
                            .show_ui(ui, |ui| {
                                for (i, name) in g.bookmarks.iter().enumerate() {
                                    ui.selectable_value(&mut g.bookmark, i, name);
                                }
                            });
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("번호");
                            let edit = ui
                                .add(egui::TextEdit::singleline(&mut g.value).desired_width(80.0));
                            if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                apply = true;
                            }
                        });
                    }
                });
                ui.vertical(|ui| {
                    let valid = g.kind == GotoKind::Bookmark
                        || g.value.trim().parse::<usize>().is_ok_and(|n| n >= 1);
                    let (a, c) = chrome::actions(ui, valid, "이동");
                    apply |= a;
                    cancel |= c;
                });
            });
        });
        if apply {
            let params = match g.kind {
                GotoKind::Bookmark => {
                    json!({"bookmark": g.bookmarks.get(g.bookmark).cloned().unwrap_or_default()})
                }
                kind => {
                    let n = g.value.trim().parse::<usize>().unwrap_or(0);
                    match kind {
                        GotoKind::Page => json!({"page": n}),
                        GotoKind::Section => json!({"section": n.saturating_sub(1)}),
                        _ => json!({"paragraph": n.saturating_sub(1)}),
                    }
                }
            };
            if self.run("edit.goto", params) {
                return;
            }
        }
        if !cancel {
            self.ext.core.goto = Some(g);
        }
    }
    fn scope_label(&self) -> &'static str {
        if self.session.span().is_some() {
            "선택한 문단"
        } else {
            "현재 구역의 모든 문단"
        }
    }
    fn sort_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut o) = self.ext.core.sort else {
            return;
        };
        let scope = self.scope_label();
        let (mut apply, mut cancel) = (false, false);
        chrome::modal("sort-dialog").show(ctx, |ui| {
            ui.set_width(380.0);
            cancel = chrome::title(ui, "정렬");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(270.0);
                    ui.label(RichText::new(format!("대상: {scope}")).weak());
                    chrome::section(ui, "정렬 기준");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut o.key, SortKey::Text, "글자 (가나다·ABC)");
                        ui.radio_value(&mut o.key, SortKey::Number, "숫자");
                    });
                    chrome::section(ui, "정렬 방식");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut o.descending, false, "오름차순");
                        ui.radio_value(&mut o.descending, true, "내림차순");
                    });
                    ui.add_enabled(
                        o.key == SortKey::Text,
                        egui::Checkbox::new(&mut o.case_sensitive, "대소문자 구별"),
                    );
                });
                ui.vertical(|ui| {
                    let (a, c) = chrome::actions(ui, true, "실행");
                    apply = a;
                    cancel |= c;
                });
            });
        });
        self.ext.core.sort = Some(o);
        if apply {
            let r = self.session.sort_paragraphs(o);
            if self.result(r).is_some() {
                self.message = "정렬했습니다. Ctrl+Z로 되돌릴 수 있습니다.".into();
                self.ext.core.sort = None;
                self.follow_cursor();
            }
        }
        if cancel {
            self.ext.core.sort = None;
        }
    }
    fn marks_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut marks) = self.ext.core.marks.take() else {
            return;
        };
        let scope = self.scope_label();
        let (mut apply, mut cancel) = (false, false);
        chrome::modal("ctrl-marks-dialog").show(ctx, |ui| {
            ui.set_width(400.0);
            cancel = chrome::title(ui, "조판 부호 지우기");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(290.0);
                    ui.label(RichText::new(format!("대상: {scope}")).weak());
                    chrome::section(ui, "지울 조판 부호");
                    egui::Grid::new("ctrl-mark-kinds")
                        .num_columns(2)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            for (i, (kind, count, checked)) in marks.iter_mut().enumerate() {
                                ui.add_enabled(
                                    *count > 0,
                                    egui::Checkbox::new(
                                        checked,
                                        format!("{} ({count})", kind.label()),
                                    ),
                                );
                                if i % 2 == 1 {
                                    ui.end_row();
                                }
                            }
                        });
                    ui.horizontal(|ui| {
                        if ui.small_button("모두 선택").clicked() {
                            marks.iter_mut().for_each(|m| m.2 = m.1 > 0);
                        }
                        if ui.small_button("모두 해제").clicked() {
                            marks.iter_mut().for_each(|m| m.2 = false);
                        }
                    });
                });
                ui.vertical(|ui| {
                    let valid = marks.iter().any(|m| m.2 && m.1 > 0);
                    let (a, c) = chrome::actions(ui, valid, "지우기");
                    apply = a;
                    cancel |= c;
                });
            });
        });
        if apply {
            let kinds: Vec<MarkKind> = marks.iter().filter(|m| m.2).map(|m| m.0).collect();
            let r = self.session.delete_ctrl_marks(&kinds);
            if let Some(n) = self.result(r) {
                self.message = format!("조판 부호 {n}개를 지웠습니다.");
                return;
            }
        }
        if !cancel {
            self.ext.core.marks = Some(marks);
        }
    }
}
