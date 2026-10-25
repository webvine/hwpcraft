use super::*;
#[derive(Clone)]
pub(super) enum FeatureDialog {
    Object(super::object_dialogs::ObjectDialog),
    HeaderFooter { is_header: bool, text: String },
    Link { uri: String, text: String },
    Bookmarks { name: String },
    Columns { count: u16, spacing: f64 },
    Search { query: String },
}
impl HwpApp {
    pub(super) fn feature_dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.feature_dialog.take() else {
            return;
        };
        let mut close = false;
        let response = chrome::modal("feature-dialog").show(ctx, |ui| {
            ui.set_width(470.0);
            match &mut draft {
                FeatureDialog::Object(draft) => close = self.object_dialog(ctx, draft, ui),
                FeatureDialog::HeaderFooter { is_header, text } => {
                    ui.heading(if *is_header { "머리말" } else { "꼬리말" });
                    ui.separator();
                    ui.label("현재 구역의 모든 쪽에 적용할 한 줄 텍스트");
                    ui.text_edit_singleline(text);
                    if ui.button("설정").clicked() {
                        close = self.run(
                            "page.header_footer",
                            json!({"is_header":is_header,"text":text}),
                        );
                    }
                }
                FeatureDialog::Link { uri, text } => {
                    ui.heading("하이퍼링크");
                    ui.separator();
                    ui.label("웹 주소 (http / https)");
                    ui.text_edit_singleline(uri);
                    if self.session.selection().is_empty() {
                        ui.label("표시할 문자열");
                        ui.text_edit_singleline(text);
                    } else {
                        ui.label(format!("선택한 문자열: {}", self.session.selected_text()));
                    }
                    ui.add_space(8.0);
                    if ui.button("넣기").clicked() {
                        close = self.run("insert.hyperlink", json!({"uri":uri,"text":text}));
                    }
                }
                FeatureDialog::Bookmarks { name } => {
                    ui.heading("책갈피");
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(name);
                        if ui
                            .add_enabled(
                                self.session.cursor.cell.is_none(),
                                egui::Button::new("추가"),
                            )
                            .clicked()
                        {
                            self.run("bookmark.add", json!({"name":name}));
                        }
                    });
                    if let Ok(list) = self.session.bookmarks() {
                        egui::ScrollArea::vertical()
                            .max_height(280.0)
                            .show(ui, |ui| {
                                for v in list.as_array().into_iter().flatten() {
                                    let label = v["name"].as_str().unwrap_or("");
                                    ui.push_id(label, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.add_sized([260.0, 24.0], egui::Label::new(label));
                                            if ui.button("이동").clicked() {
                                                close = self
                                                    .run("bookmark.goto", json!({"name":label}));
                                            }
                                            if ui.button("삭제").clicked() {
                                                self.run("bookmark.delete", json!({"name":label}));
                                            }
                                        });
                                    });
                                }
                            });
                    }
                }
                FeatureDialog::Columns { count, spacing } => {
                    ui.heading("다단 설정");
                    ui.separator();
                    ui.label("현재 구역에 같은 너비의 일반 다단을 적용합니다.");
                    ui.horizontal(|ui| {
                        ui.label("단 수");
                        ui.add(egui::DragValue::new(count).range(1..=6));
                        ui.label("단 사이");
                        ui.add(
                            egui::DragValue::new(spacing)
                                .range(0.0..=50.0)
                                .suffix(" mm"),
                        );
                    });
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(440.0, 110.0), egui::Sense::hover());
                    for n in 0..*count {
                        let width = rect.width() / f32::from(*count);
                        let r = Rect::from_min_size(
                            rect.min + Vec2::new(f32::from(n) * width, 0.0),
                            Vec2::new(width - 8.0, rect.height()),
                        );
                        ui.painter().rect_filled(r, 0.0, Color32::from_gray(236));
                    }
                    if ui.button("설정").clicked() {
                        close =
                            self.run("page.columns", json!({"count":count,"spacing_mm":spacing}));
                    }
                }
                FeatureDialog::Search { query } => {
                    ui.heading("명령 찾기");
                    ui.text_edit_singleline(query);
                    ui.separator();
                    close = self.command_search_results(ui, query);
                }
            }
            ui.separator();
            if ui.button("닫기").clicked() {
                close = true;
            }
        });
        if !close && !response.should_close() {
            self.feature_dialog = Some(draft);
        }
    }
}
