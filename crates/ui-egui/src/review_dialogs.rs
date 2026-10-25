//! Owner: review-tools area. 검토·도구 UI: ribbon routing for 한자 입력 (F9) and 문서 비교,
//! plus the modeless 문서 비교 result window (검토 탭).
//! Other areas must not edit this file; request changes through the owner.
use super::*;
use serde_json::Value;
use std::path::PathBuf;

/// Dialog state owned by this module. Stored in `HwpApp::ext.tools`.
#[derive(Default)]
pub(crate) struct State {
    pub hanja: Option<hanja_dialog::HanjaDialog>,
    pub compare: Option<CompareWindow>,
    pub autotext: Option<autotext_dialog::AutoTextDialog>,
}

/// Result of `review.compare`, shown until the user closes it.
pub(crate) struct CompareWindow {
    pub path: PathBuf,
    pub ignore_whitespace: bool,
    pub result: Value,
    pub selected: Option<usize>,
}

impl HwpApp {
    /// `Some(enabled)` when this module owns the ribbon/menu command id.
    pub(crate) fn tools_ext_enabled(&self, id: &str) -> Option<bool> {
        Some(match id {
            "insert.hanja"
            | "review.compare"
            | "tools.autotext"
            | "tools.autotext.register"
            | "tools.autotext.expand" => true,
            "review.compare.next" | "review.compare.prev" => self.ext.tools.compare.is_some(),
            _ => return None,
        })
    }
    /// Return true when the command id was handled.
    pub(crate) fn tools_ext_dispatch(&mut self, id: &str, _ctx: &egui::Context) -> bool {
        match id {
            "insert.hanja" => self.open_hanja_dialog(),
            "review.compare" => self.open_compare(),
            "review.compare.next" => self.compare_step(1),
            "review.compare.prev" => self.compare_step(-1),
            "tools.autotext" => self.open_autotext_contents(),
            "tools.autotext.register" => self.open_autotext_register(),
            "tools.autotext.expand" => self.expand_autotext(),
            _ => return false,
        }
        true
    }
    /// Called every frame after the built-in dialogs.
    pub(crate) fn tools_ext_dialogs(&mut self, ctx: &egui::Context) {
        self.hanja_dialog(ctx);
        self.autotext_dialog(ctx);
        self.compare_window(ctx);
    }
    /// F9 opens 한자로 바꾸기 while the document has focus; true when consumed.
    pub(crate) fn tools_ext_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        let focus = ctx.memory(|m| m.focused());
        if focus.is_some_and(|f| f != egui::Id::new("document-canvas")) {
            return false;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F9)) {
            self.open_hanja_dialog();
            return true;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::F3)) {
            self.open_autotext_contents();
            return true;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, Key::I)) {
            self.expand_autotext();
            return true;
        }
        false
    }
    /// Start-up `--view` names: `hanja` opens 한자로 바꾸기 for the word at the caret,
    /// `compare:<path>` runs 문서 비교 against that file.
    pub(crate) fn tools_ext_view(&mut self, name: &str) -> bool {
        if name == "hanja" {
            self.open_hanja_dialog();
            return true;
        }
        if name == "autotext" {
            self.open_autotext_contents();
            return true;
        }
        if name == "autotext-register" {
            self.open_autotext_register();
            return true;
        }
        if let Some(path) = name.strip_prefix("compare:") {
            self.run_compare(PathBuf::from(path), false);
            return true;
        }
        false
    }
    /// True while a modal dialog of this module blocks typing and shortcuts.
    pub(crate) fn tools_ext_modal_open(&self) -> bool {
        self.ext.tools.hanja.is_some() || self.ext.tools.autotext.is_some()
    }

    /// 문서 비교: pick a file, run `review.compare`, open the result window.
    fn open_compare(&mut self) {
        let mut picker = rfd::FileDialog::new()
            .set_title("비교할 문서")
            .add_filter("한글 문서 (*.hwp, *.hwpx, *.hml)", &["hwp", "hwpx", "hml"]);
        if let Some(dir) = self.session.path.as_deref().and_then(|p| p.parent()) {
            picker = picker.set_directory(dir);
        }
        let Some(path) = picker.pick_file() else {
            return;
        };
        let ignore_whitespace = self
            .ext
            .tools
            .compare
            .as_ref()
            .is_some_and(|c| c.ignore_whitespace);
        self.run_compare(path, ignore_whitespace);
    }
    fn run_compare(&mut self, path: PathBuf, ignore_whitespace: bool) {
        let r = self.session.run(
            "review.compare",
            json!({"path": path, "ignore_whitespace": ignore_whitespace}),
        );
        let Some(result) = self.result(r) else {
            return;
        };
        let total = result["total"].as_u64().unwrap_or(0);
        self.message = if result["identical"].as_bool() == Some(true) {
            "두 문서의 본문·표·스타일이 같습니다.".into()
        } else {
            format!("문서 비교: 차이 {total}건")
        };
        self.ext.tools.compare = Some(CompareWindow {
            path,
            ignore_whitespace,
            result,
            selected: None,
        });
    }
    /// Move to the next/previous finding that points into the current document.
    fn compare_step(&mut self, delta: i32) {
        let Some(window) = self.ext.tools.compare.as_mut() else {
            return;
        };
        let findings = window.result["findings"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if findings.is_empty() {
            return;
        }
        let n = findings.len() as i32;
        let mut i = window
            .selected
            .map(|s| s as i32)
            .unwrap_or(if delta > 0 { -1 } else { n });
        for _ in 0..n {
            i = (i + delta).rem_euclid(n);
            if !findings[i as usize]["cursor"].is_null() {
                window.selected = Some(i as usize);
                self.goto_finding(&findings[i as usize]);
                return;
            }
        }
    }
    fn goto_finding(&mut self, finding: &Value) {
        let Ok(cursor) = serde_json::from_value::<Cursor>(finding["cursor"].clone()) else {
            return;
        };
        let r = self.session.select(cursor, false);
        if self.result(r).is_some() {
            self.sync_formatting();
            self.follow_cursor();
        }
    }

    fn compare_window(&mut self, ctx: &egui::Context) {
        let Some(window) = self.ext.tools.compare.as_ref() else {
            return;
        };
        let path = window.path.clone();
        let mut ignore_whitespace = window.ignore_whitespace;
        let result = window.result.clone();
        let mut selected = window.selected;
        let mut close = false;
        let mut rerun = false;
        let mut step = 0;
        let mut goto: Option<Value> = None;
        egui::Window::new("문서 비교")
            .id(egui::Id::new("hwp-compare-window"))
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size(Vec2::new(520.0, 420.0))
            .default_pos(ctx.content_rect().right_top() + Vec2::new(-560.0, 120.0))
            .frame(
                egui::Frame::window(&ctx.global_style())
                    .fill(chrome::Tokens::PANEL)
                    .stroke(Stroke::new(1.0, chrome::Tokens::BORDER))
                    .corner_radius(0),
            )
            .show(ctx, |ui| {
                close |= chrome::title(ui, "문서 비교");
                ui.horizontal(|ui| {
                    ui.label("비교 파일:");
                    ui.add(
                        egui::Label::new(RichText::new(path.display().to_string()).small())
                            .truncate(),
                    );
                });
                ui.horizontal(|ui| {
                    rerun |= ui
                        .checkbox(&mut ignore_whitespace, "띄어쓰기 차이 무시")
                        .changed();
                    if ui.button("다시 비교").clicked() {
                        rerun = true;
                    }
                });
                let summary: Vec<String> = result["summary"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .map(|s| {
                                format!(
                                    "{} {}",
                                    s["label"].as_str().unwrap_or(""),
                                    s["count"].as_u64().unwrap_or(0)
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let total = result["total"].as_u64().unwrap_or(0);
                let status = if result["identical"].as_bool() == Some(true) {
                    "차이가 없습니다.".to_owned()
                } else if result["truncated"].as_bool() == Some(true) {
                    format!("차이 {total}건 (상한에 걸려 일부만 표시) — {}", summary.join(", "))
                } else {
                    format!("차이 {total}건 — {}", summary.join(", "))
                };
                ui.label(RichText::new(status).small().color(Color32::DARK_GRAY));
                ui.add_space(4.0);
                let findings = result["findings"].as_array().cloned().unwrap_or_default();
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Grid::new("compare-findings")
                            .num_columns(3)
                            .striped(true)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                ui.strong("위치");
                                ui.strong("종류");
                                ui.strong("내용");
                                ui.end_row();
                                for (i, f) in findings.iter().enumerate() {
                                    let has_cursor = !f["cursor"].is_null();
                                    let is_selected = selected == Some(i);
                                    let place = match (f["section"].as_u64(), f["paragraph"].as_u64()) {
                                        (Some(s), Some(p)) => format!("{}구역 {}문단", s + 1, p + 1),
                                        (Some(s), None) => format!("{}구역", s + 1),
                                        _ => "문서".to_owned(),
                                    };
                                    let r = ui.selectable_label(is_selected, place);
                                    ui.label(f["label"].as_str().unwrap_or(""));
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(f["detail"].as_str().unwrap_or(""))
                                                .small(),
                                        )
                                        .truncate(),
                                    );
                                    ui.end_row();
                                    if r.clicked() {
                                        selected = Some(i);
                                        if has_cursor {
                                            goto = Some(f.clone());
                                        }
                                    }
                                }
                            });
                    });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "현재 문서를 수정본, 선택한 파일을 원본으로 봅니다. 본문·표 셀의 문단과 표·개체·스타일을 비교하며 글상자·각주·머리말 안은 비교하지 않습니다.",
                    )
                    .small()
                    .color(Color32::GRAY),
                );
                ui.horizontal(|ui| {
                    if ui.button("이전 차이").clicked() {
                        step = -1;
                    }
                    if ui.button("다음 차이").clicked() {
                        step = 1;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("닫기").clicked() {
                            close = true;
                        }
                    });
                });
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    close = true;
                }
            });
        if let Some(window) = self.ext.tools.compare.as_mut() {
            window.selected = selected;
            window.ignore_whitespace = ignore_whitespace;
        }
        if let Some(f) = goto {
            self.goto_finding(&f);
        }
        if step != 0 {
            self.compare_step(step);
        }
        if rerun {
            self.run_compare(path, ignore_whitespace);
        }
        if close {
            self.ext.tools.compare = None;
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("document-canvas")));
        }
    }
}
