//! Owner: core area. Core editing tools on the ribbon: 정렬, 조판 부호 지우기, 한자 변환.
use super::*;
use hwpcraft_engine::{MarkKind, SortKey, SortOptions};

#[derive(Default)]
pub(crate) struct State {
    sort: Option<SortOptions>,
    marks: Option<Vec<(MarkKind, usize, bool)>>,
}

impl HwpApp {
    pub(crate) fn core_ext_enabled(&self, id: &str) -> Option<bool> {
        match id {
            // Like Hangul, 정렬 needs a block of paragraphs.
            "edit.sort" => {
                Some(self.session.cursor.cell.is_none() && self.session.span().is_some())
            }
            "edit.delete_ctrl_marks" => Some(self.session.cursor.cell.is_none()),
            // 한자 변환 needs a Hanja dictionary that HwpCraft does not ship yet.
            "insert.hanja" => Some(false),
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
            _ => return false,
        }
        true
    }
    pub(crate) fn core_ext_modal_open(&self) -> bool {
        self.ext.core.sort.is_some() || self.ext.core.marks.is_some()
    }
    pub(crate) fn core_ext_view(&mut self, name: &str) -> bool {
        match name {
            "sort" => self.core_ext_dispatch("edit.sort"),
            "ctrl-marks" => self.core_ext_dispatch("edit.delete_ctrl_marks"),
            _ => false,
        }
    }
    pub(crate) fn core_ext_dialogs(&mut self, ctx: &egui::Context) {
        self.sort_dialog(ctx);
        self.marks_dialog(ctx);
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
