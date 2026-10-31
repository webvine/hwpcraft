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
    /// `review.merge.plan`: each difference with both texts, for 나란히 보기·병합.
    pub plan: Value,
    /// Ids of the differences ticked for 병합.
    pub picked: std::collections::BTreeSet<usize>,
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
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::F3)) {
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
        let plan = self
            .session
            .run(
                "review.merge.plan",
                json!({"path": path, "ignore_whitespace": ignore_whitespace}),
            )
            .unwrap_or_default();
        self.ext.tools.compare = Some(CompareWindow {
            path,
            ignore_whitespace,
            result,
            selected: None,
            plan,
            picked: Default::default(),
        });
    }

    /// Take the ticked differences (all of them with `None`) over from the file.
    fn merge_compare(&mut self, ids: Option<Vec<usize>>) {
        let Some(window) = self.ext.tools.compare.as_ref() else {
            return;
        };
        let (path, ignore_whitespace) = (window.path.clone(), window.ignore_whitespace);
        let mut params = json!({"path": path, "ignore_whitespace": ignore_whitespace});
        if let Some(ids) = ids {
            params["ids"] = json!(ids);
        }
        let r = self.session.run("review.merge", params);
        if let Some(done) = self.result(r) {
            self.sync_formatting();
            self.follow_cursor();
            self.run_compare(path, ignore_whitespace);
            self.message = format!(
                "병합: {}건 가져옴, 남은 차이 {}건",
                done["applied"].as_u64().unwrap_or(0),
                done["changes"].as_u64().unwrap_or(0)
            );
        }
    }
    /// Move to the next/previous finding that points into the current document.
    fn compare_step(&mut self, delta: i32) {
        let Some(window) = self.ext.tools.compare.as_mut() else {
            return;
        };
        let Some(findings) = window.result["findings"].as_array() else {
            return;
        };
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
                let finding = findings[i as usize].clone();
                self.goto_finding(&finding);
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
        // Taken for the frame and put back below, so the result is drawn by reference.
        let Some(window) = self.ext.tools.compare.take() else {
            return;
        };
        let path = window.path.clone();
        let mut ignore_whitespace = window.ignore_whitespace;
        let result = &window.result;
        let mut selected = window.selected;
        let mut picked = window.picked.clone();
        let mut merge: Option<Option<Vec<usize>>> = None;
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
            // Below the ribbon, format bar and ruler, clear of the vertical scroll bar.
            .default_pos(ctx.content_rect().right_top() + Vec2::new(-560.0, 190.0))
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
                ui.label(RichText::new(status).size(12.5).color(Color32::DARK_GRAY));
                ui.add_space(4.0);
                let findings: &[Value] = result["findings"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let navigable = findings.iter().any(|f| !f["cursor"].is_null());
                // Fixed column widths keep the header over its column; only the
                // visible rows are laid out, however many findings there are.
                const PLACE: f32 = 96.0;
                const KIND: f32 = 196.0;
                let row_height = ui.spacing().interact_size.y + 2.0;
                // A left-aligned cell of fixed width.
                let column = |ui: &mut egui::Ui, width: f32, widget: egui::Label| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(width, row_height),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_width(width);
                            ui.add(widget)
                        },
                    )
                    .inner
                };
                ui.horizontal(|ui| {
                    column(ui, PLACE, egui::Label::new(RichText::new("위치").strong()));
                    column(ui, KIND, egui::Label::new(RichText::new("종류").strong()));
                    ui.label(RichText::new("내용").strong());
                });
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .auto_shrink([false, false])
                    .show_rows(ui, row_height, findings.len(), |ui, rows| {
                        for (i, f) in findings.iter().enumerate().skip(rows.start).take(rows.len()) {
                            let has_cursor = !f["cursor"].is_null();
                            let place = match (f["section"].as_u64(), f["paragraph"].as_u64()) {
                                (Some(s), Some(p)) => format!("{}구역 {}문단", s + 1, p + 1),
                                (Some(s), None) => format!("{}구역", s + 1),
                                _ => "문서".to_owned(),
                            };
                            ui.horizontal(|ui| {
                                let r = ui.add_sized(
                                    [PLACE, row_height],
                                    egui::Button::selectable(selected == Some(i), place),
                                );
                                column(
                                    ui,
                                    KIND,
                                    egui::Label::new(f["label"].as_str().unwrap_or("")).truncate(),
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(f["text"].as_str().or(f["detail"].as_str()).unwrap_or("")).size(12.0),
                                    )
                                    .truncate(),
                                );
                                if r.clicked() {
                                    selected = Some(i);
                                    if has_cursor {
                                        goto = Some(f.clone());
                                    }
                                }
                            });
                        }
                    });
                let changes: &[Value] = window.plan["changes"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                egui::CollapsingHeader::new(format!("나란히 보기·병합 ({}건)", changes.len()))
                    .id_salt("compare-merge")
                    .default_open(true)
                    .show(ui, |ui| {
                        const TICK: f32 = 24.0;
                        const WHAT: f32 = 56.0;
                        let text = (ui.available_width() - TICK - WHAT - 16.0).max(120.0) / 2.0;
                        ui.horizontal(|ui| {
                            column(ui, TICK, egui::Label::new(""));
                            column(ui, WHAT, egui::Label::new(RichText::new("종류").strong()));
                            column(ui, text, egui::Label::new(RichText::new("현재 문서").strong()));
                            ui.label(RichText::new("비교 파일").strong());
                        });
                        egui::ScrollArea::vertical()
                            .id_salt("compare-merge-rows")
                            .max_height(180.0)
                            .auto_shrink([false, true])
                            .show_rows(ui, row_height, changes.len(), |ui, rows| {
                                for c in changes.iter().skip(rows.start).take(rows.len()) {
                                    let Some(id) = c["id"].as_u64().map(|v| v as usize) else {
                                        continue;
                                    };
                                    let what = match c["kind"].as_str() {
                                        Some("changed") => "바뀜",
                                        Some("added") => "파일에만",
                                        _ => "현재에만",
                                    };
                                    let shown = |v: &Value| match v.as_str() {
                                        None => "—".to_owned(),
                                        Some("") => "(표·개체 문단)".to_owned(),
                                        Some(t) => t.to_owned(),
                                    };
                                    ui.horizontal(|ui| {
                                        let mut on = picked.contains(&id);
                                        if ui.checkbox(&mut on, "").changed() {
                                            if on {
                                                picked.insert(id);
                                            } else {
                                                picked.remove(&id);
                                            }
                                        }
                                        column(ui, WHAT, egui::Label::new(what));
                                        column(
                                            ui,
                                            text,
                                            egui::Label::new(shown(&c["current_text"])).truncate(),
                                        );
                                        ui.add(egui::Label::new(shown(&c["other_text"])).truncate());
                                    });
                                }
                            });
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(!picked.is_empty(), egui::Button::new("고른 변경 병합"))
                                .on_disabled_hover_text("병합할 차이를 고르세요.")
                                .clicked()
                            {
                                merge = Some(Some(picked.iter().copied().collect()));
                            }
                            if ui
                                .add_enabled(!changes.is_empty(), egui::Button::new("모두 병합"))
                                .on_disabled_hover_text("가져올 차이가 없습니다.")
                                .clicked()
                            {
                                merge = Some(None);
                            }
                        });
                        ui.label(
                            RichText::new(
                                "병합은 고른 차이를 비교 파일 쪽으로 맞춥니다(바뀐 문단은 파일 문단으로, 파일에만 있는 문단은 넣고, 현재에만 있는 문단은 지움). 서식·표·그림도 함께 가져오며 한 번에 되돌립니다.",
                            )
                            .size(12.0)
                            .color(Color32::GRAY),
                        );
                    });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "현재 문서를 수정본, 선택한 파일을 원본으로 봅니다. 본문·표 셀의 문단과 표·개체·스타일을 비교하며 글상자·각주·머리말 안은 비교하지 않습니다.",
                    )
                    .size(12.0)
                    .color(Color32::GRAY),
                );
                ui.horizontal(|ui| {
                    let reason = "현재 문서 안을 가리키는 차이가 없습니다(비교 파일에만 있는 문단 등).";
                    if ui
                        .add_enabled(navigable, egui::Button::new("이전 차이"))
                        .on_disabled_hover_text(reason)
                        .clicked()
                    {
                        step = -1;
                    }
                    if ui
                        .add_enabled(navigable, egui::Button::new("다음 차이"))
                        .on_disabled_hover_text(reason)
                        .clicked()
                    {
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
        self.ext.tools.compare = Some(CompareWindow {
            selected,
            ignore_whitespace,
            picked,
            ..window
        });
        if let Some(ids) = merge {
            self.merge_compare(ids);
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
