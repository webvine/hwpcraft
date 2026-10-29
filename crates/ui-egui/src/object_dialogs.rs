use super::*;
use hwpcraft_engine::{ObjectAddress, ShapeKind, ShapeOptions};

#[derive(Clone)]
pub(super) enum ObjectDialog {
    /// Opens the full 수식 편집기 (ext_object) for a new or existing equation.
    Equation {
        address: Option<ObjectAddress>,
    },
    Note {
        address: Option<ObjectAddress>,
        endnote: bool,
        text: String,
    },
    Shape(ShapeOptions),
    Resize {
        address: ObjectAddress,
        width: f64,
        height: f64,
    },
    Textbox {
        address: ObjectAddress,
        text: String,
    },
    Objects,
    Styles,
}
impl ObjectDialog {
    pub(super) fn equation() -> Self {
        Self::Equation { address: None }
    }
    pub(super) fn shape(kind: ShapeKind) -> Self {
        Self::Shape(ShapeOptions {
            kind,
            width_mm: 60.0,
            height_mm: 25.0,
            text: String::new(),
        })
    }
}
impl HwpApp {
    /// Object dialogs in the shared dialog frame (title bar with ×, right-hand
    /// buttons, Enter = default, Esc = last button). Returns true to stay open.
    pub(super) fn object_dialog_window(
        &mut self,
        ctx: &egui::Context,
        draft: &mut ObjectDialog,
    ) -> bool {
        use super::ext_shell::{dialog, group, note, notice};
        match draft {
            ObjectDialog::Equation { address } => {
                // The full 수식 편집기 lives in ext_object; hand over and close this one.
                self.open_equation_editor(*address);
                false
            }
            ObjectDialog::Note {
                address,
                endnote,
                text,
            } => {
                let title = if *endnote { "미주" } else { "각주" };
                let primary = if address.is_some() {
                    "고치기(D)"
                } else {
                    "넣기(D)"
                };
                let button = dialog(ctx, title, 480.0, &[primary, "취소"], |ui| {
                    group(
                        ui,
                        if *endnote {
                            "구역 끝에 표시할 주석 내용"
                        } else {
                            "쪽 아래에 표시할 주석 내용"
                        },
                    );
                    ui.add(
                        egui::TextEdit::multiline(text)
                            .desired_rows(5)
                            .desired_width(f32::INFINITY),
                    );
                    note(ui, "자동 번호를 유지합니다. Enter로 문단을 나눕니다.");
                });
                match button {
                    Some(0) => {
                        let paragraphs: Vec<&str> = text.split('\n').collect();
                        !if let Some(address) = address {
                            self.run(
                                "note.paragraphs",
                                json!({"address":address,"paragraphs":paragraphs}),
                            )
                        } else {
                            self.run(
                                "note.insert",
                                json!({"endnote":*endnote,"paragraphs":paragraphs}),
                            )
                        }
                    }
                    Some(_) => false,
                    None => true,
                }
            }
            ObjectDialog::Shape(p) => {
                let button = dialog(
                    ctx,
                    "도형 / 글상자 넣기",
                    520.0,
                    &["넣기(D)", "취소"],
                    |ui| {
                        group(ui, "종류");
                        ui.horizontal(|ui| {
                            for (kind, label) in [
                                (ShapeKind::Rectangle, "사각형"),
                                (ShapeKind::Ellipse, "타원"),
                                (ShapeKind::Line, "선"),
                                (ShapeKind::Textbox, "글상자"),
                            ] {
                                if ui.selectable_value(&mut p.kind, kind, label).changed()
                                    && kind != ShapeKind::Textbox
                                {
                                    p.text.clear();
                                }
                            }
                        });
                        group(ui, "크기");
                        size_fields(ui, &mut p.width_mm, &mut p.height_mm);
                        shape_preview(ui, p);
                        if p.kind == ShapeKind::Textbox {
                            group(ui, "글상자 내용");
                            ui.add(
                                egui::TextEdit::singleline(&mut p.text)
                                    .desired_width(f32::INFINITY),
                            );
                        }
                        notice(
                            ui,
                            "커서 위치에 글자처럼 취급하는 개체로 넣습니다. 쪽의 원하는 자리에 그리려면 입력 탭의 도형 단추로 끌어 그리세요.",
                        );
                    },
                );
                match button {
                    Some(0) => !self.run("insert.shape", json!(p)),
                    Some(_) => false,
                    None => true,
                }
            }
            ObjectDialog::Resize {
                address,
                width,
                height,
            } => {
                let button = dialog(ctx, "개체 크기", 420.0, &["설정(D)", "취소"], |ui| {
                    group(ui, "크기");
                    size_fields(ui, width, height);
                });
                match button {
                    Some(0) => !self.run(
                        "shape.resize",
                        json!({"address":address,"width_mm":width,"height_mm":height}),
                    ),
                    Some(_) => false,
                    None => true,
                }
            }
            ObjectDialog::Textbox { address, text } => {
                let button = dialog(
                    ctx,
                    "글상자 내용 고치기",
                    480.0,
                    &["고치기(D)", "취소"],
                    |ui| {
                        group(ui, "내용");
                        ui.add(
                            egui::TextEdit::multiline(text)
                                .desired_rows(6)
                                .desired_width(f32::INFINITY),
                        );
                        note(ui, "Enter로 문단을 나눕니다.");
                    },
                );
                match button {
                    Some(0) => {
                        let paragraphs: Vec<&str> = text.split('\n').collect();
                        !self.run(
                            "textbox.paragraphs",
                            json!({"address":address,"paragraphs":paragraphs}),
                        )
                    }
                    Some(_) => false,
                    None => true,
                }
            }
            ObjectDialog::Styles => {
                let styles = self.session.styles();
                let mut chosen = None;
                let button = dialog(ctx, "스타일", 420.0, &["닫기"], |ui| {
                    group(ui, "문서에 저장된 스타일");
                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .show(ui, |ui| {
                            for s in styles.as_array().into_iter().flatten() {
                                ui.horizontal(|ui| {
                                    ui.weak(if s["character"] == true {
                                        "글자"
                                    } else {
                                        "문단"
                                    });
                                    if ui
                                        .button(s["name"].as_str().unwrap_or("이름 없음"))
                                        .clicked()
                                    {
                                        chosen = Some(s["id"].clone());
                                    }
                                });
                            }
                        });
                    note(ui, "누른 스타일을 현재 문단에 적용합니다.");
                });
                if let Some(id) = chosen {
                    return !self.run("style.apply", json!({"id":id}));
                }
                button.is_none()
            }
            ObjectDialog::Objects => {
                let objects = self.session.objects();
                let mut action: Option<(ObjectAddress, &'static str)> = None;
                let button = dialog(ctx, "개체 목록", 680.0, &["닫기"], |ui| {
                    group(ui, "본문의 그림 · 표 · 수식 · 도형 · 글상자 · 각주 · 미주");
                    if objects.as_array().is_none_or(|v| v.is_empty()) {
                        notice(ui, "이 문서에 편집할 개체가 없습니다.");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(380.0)
                        .show(ui, |ui| {
                            egui::Grid::new("object-list-window")
                                .striped(true)
                                .num_columns(4)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    for (i, item) in
                                        objects.as_array().into_iter().flatten().enumerate()
                                    {
                                        let Ok(address) = serde_json::from_value::<ObjectAddress>(
                                            item["address"].clone(),
                                        ) else {
                                            continue;
                                        };
                                        let kind = item["kind"].as_str().unwrap_or("");
                                        ui.strong(object_kind_label(kind));
                                        ui.weak(format!(
                                            "{}구역 {}문단",
                                            address.section + 1,
                                            address.paragraph + 1
                                        ));
                                        let full = item["label"].as_str().unwrap_or("");
                                        let short: String = full.chars().take(24).collect();
                                        let shown = if full.chars().count() > 24 {
                                            format!("{short}…")
                                        } else {
                                            short
                                        };
                                        ui.label(shown).on_hover_text(full);
                                        ui.push_id(i, |ui| {
                                            ui.horizontal(|ui| {
                                                for (label, id) in object_actions(kind) {
                                                    if ui.button(*label).clicked() {
                                                        action = Some((address, id));
                                                    }
                                                }
                                            });
                                        });
                                        ui.end_row();
                                    }
                                });
                        });
                });
                if let Some((address, id)) = action {
                    return self.object_list_action(address, id);
                }
                button.is_none()
            }
        }
    }

    /// One row action of the object list; returns true to keep the list open.
    fn object_list_action(&mut self, address: ObjectAddress, id: &str) -> bool {
        let kind = self
            .session
            .objects()
            .as_array()
            .into_iter()
            .flatten()
            .find(|o| {
                serde_json::from_value::<ObjectAddress>(o["address"].clone()).ok() == Some(address)
            })
            .and_then(|o| o["kind"].as_str().map(str::to_owned))
            .unwrap_or_default();
        let next = match id {
            "properties" => {
                self.open_object_properties(address);
                return false;
            }
            "ungroup" => return !self.run("object.ungroup", json!({"address":address})),
            "form" => {
                self.open_form_editor();
                return false;
            }
            "delete" => {
                // The list stays open and shows the remaining objects.
                self.run("object.delete", json!(address));
                return true;
            }
            "size" => {
                self.session
                    .shape_size(address)
                    .map(|(width, height)| ObjectDialog::Resize {
                        address,
                        width,
                        height,
                    })
            }
            _ => match kind.as_str() {
                "equation" => Ok(ObjectDialog::Equation {
                    address: Some(address),
                }),
                "textbox" => {
                    self.session
                        .textbox_paragraphs(address)
                        .map(|p| ObjectDialog::Textbox {
                            address,
                            text: p.join("\n"),
                        })
                }
                _ => self
                    .session
                    .note_paragraphs(address)
                    .map(|p| ObjectDialog::Note {
                        address: Some(address),
                        endnote: kind == "endnote",
                        text: p.join("\n"),
                    }),
            },
        };
        if let Some(next) = self.result(next) {
            self.feature_dialog = Some(feature_dialogs::FeatureDialog::Object(next));
        }
        false
    }
}

fn object_kind_label(kind: &str) -> &'static str {
    match kind {
        "equation" => "수식",
        "footnote" => "각주",
        "endnote" => "미주",
        "textbox" => "글상자",
        "picture" => "그림",
        "table" => "표",
        "group" => "묶음 개체",
        "form" => "양식 개체",
        _ => "도형",
    }
}
/// Row buttons of the object list per kind: (label, action id). 지우기 is last.
fn object_actions(kind: &str) -> &'static [(&'static str, &'static str)] {
    match kind {
        "picture" | "shape" => &[
            ("속성", "properties"),
            ("크기", "size"),
            ("지우기", "delete"),
        ],
        "textbox" => &[
            ("속성", "properties"),
            ("내용 고치기", "edit"),
            ("크기", "size"),
            ("지우기", "delete"),
        ],
        "equation" => &[
            ("속성", "properties"),
            ("내용 고치기", "edit"),
            ("지우기", "delete"),
        ],
        "group" => &[("풀기", "ungroup"), ("지우기", "delete")],
        "form" => &[("값 고치기", "form"), ("지우기", "delete")],
        "table" => &[("지우기", "delete")],
        _ => &[("내용 고치기", "edit"), ("지우기", "delete")],
    }
}
fn shape_preview(ui: &mut egui::Ui, p: &ShapeOptions) {
    // Fit the dialog's body width so the shared frame keeps its button column.
    let width = ui.available_width().clamp(200.0, 440.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(width, 120.0), egui::Sense::hover());
    let preview = Rect::from_center_size(
        r.center(),
        Vec2::new((p.width_mm * 2.0) as f32, (p.height_mm * 2.0) as f32)
            .min(r.size() - Vec2::splat(20.0)),
    );
    let stroke = Stroke::new(1.5, Color32::from_rgb(60, 115, 180));
    match p.kind {
        ShapeKind::Line => {
            ui.painter()
                .line_segment([preview.left_top(), preview.right_bottom()], stroke);
        }
        ShapeKind::Ellipse => {
            ui.painter().add(egui::epaint::EllipseShape::stroke(
                preview.center(),
                preview.size() * 0.5,
                stroke,
            ));
        }
        _ => {
            ui.painter()
                .rect_stroke(preview, 0.0, stroke, egui::StrokeKind::Inside);
        }
    }
}
fn size_fields(ui: &mut egui::Ui, width: &mut f64, height: &mut f64) {
    ui.horizontal(|ui| {
        ui.label("너비");
        ui.add(
            egui::DragValue::new(width)
                .range(1.0..=300.0)
                .fixed_decimals(2)
                .suffix(" mm"),
        );
        ui.label("높이");
        ui.add(
            egui::DragValue::new(height)
                .range(1.0..=300.0)
                .fixed_decimals(2)
                .suffix(" mm"),
        );
    });
}
