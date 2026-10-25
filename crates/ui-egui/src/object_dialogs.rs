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
    Symbols {
        category: usize,
        text: String,
    },
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
    pub(super) fn object_dialog(
        &mut self,
        _ctx: &egui::Context,
        draft: &mut ObjectDialog,
        ui: &mut egui::Ui,
    ) -> bool {
        let mut close = false;
        match draft {
            ObjectDialog::Equation { address } => {
                // The full 수식 편집기 lives in ext_object; hand over and close this popup.
                self.open_equation_editor(*address);
                return true;
            }
            ObjectDialog::Note {
                address,
                endnote,
                text,
            } => {
                ui.heading(if *endnote { "미주" } else { "각주" });
                ui.separator();
                ui.label(if *endnote {
                    "구역 끝에 표시할 주석 내용"
                } else {
                    "쪽 아래에 표시할 주석 내용"
                });
                ui.add(
                    egui::TextEdit::multiline(text)
                        .desired_rows(5)
                        .desired_width(f32::INFINITY),
                );
                ui.label("자동 번호를 유지합니다. Enter로 문단을 나눕니다.");
                if ui
                    .button(if address.is_some() {
                        "고치기"
                    } else {
                        "넣기"
                    })
                    .clicked()
                {
                    let paragraphs: Vec<&str> = text.split('\n').collect();
                    close = if let Some(address) = address {
                        self.run(
                            "note.paragraphs",
                            json!({"address":address,"paragraphs":paragraphs}),
                        )
                    } else {
                        self.run(
                            "note.insert",
                            json!({"endnote":*endnote,"paragraphs":paragraphs}),
                        )
                    };
                }
            }
            ObjectDialog::Shape(p) => {
                ui.heading("도형 / 글상자 넣기");
                ui.separator();
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
                size_fields(ui, &mut p.width_mm, &mut p.height_mm);
                let (r, _) = ui.allocate_exact_size(Vec2::new(440.0, 120.0), egui::Sense::hover());
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
                if p.kind == ShapeKind::Textbox {
                    ui.label("글상자 내용");
                    ui.add(egui::TextEdit::singleline(&mut p.text).desired_width(f32::INFINITY));
                }
                ui.label("본문에 글자처럼 취급하는 개체로 넣습니다.");
                if ui.button("넣기").clicked() {
                    close = self.run("insert.shape", json!(p));
                }
            }
            ObjectDialog::Resize {
                address,
                width,
                height,
            } => {
                ui.heading("개체 크기");
                ui.separator();
                size_fields(ui, width, height);
                if ui.button("설정").clicked() {
                    close = self.run(
                        "shape.resize",
                        json!({"address":address,"width_mm":width,"height_mm":height}),
                    );
                }
            }
            ObjectDialog::Textbox { address, text } => {
                ui.heading("글상자 내용 고치기");
                ui.separator();
                ui.add(
                    egui::TextEdit::multiline(text)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY),
                );
                ui.label("Enter로 문단을 나눕니다.");
                if ui.button("고치기").clicked() {
                    let paragraphs: Vec<&str> = text.split('\n').collect();
                    close = self.run(
                        "textbox.paragraphs",
                        json!({"address":address,"paragraphs":paragraphs}),
                    );
                }
            }
            ObjectDialog::Styles => {
                ui.heading("스타일");
                ui.separator();
                ui.label("문서에 저장된 스타일을 현재 문단에 적용합니다.");
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        for s in self.session.styles().as_array().into_iter().flatten() {
                            ui.horizontal(|ui| {
                                ui.label(if s["character"] == true {
                                    "글자"
                                } else {
                                    "문단"
                                });
                                if ui
                                    .button(s["name"].as_str().unwrap_or("이름 없음"))
                                    .clicked()
                                {
                                    close = self.run("style.apply", json!({"id":s["id"]}));
                                }
                            });
                        }
                    });
            }
            ObjectDialog::Objects => {
                ui.heading("개체 목록");
                ui.separator();
                ui.label("본문의 그림 · 표 · 수식 · 도형 · 글상자 · 각주 · 미주");
                let objects = self.session.objects();
                if objects.as_array().is_none_or(|v| v.is_empty()) {
                    ui.label("이 문서에 편집할 개체가 없습니다.");
                }
                egui::ScrollArea::vertical()
                    .max_height(380.0)
                    .show(ui, |ui| {
                        for (i, item) in objects.as_array().into_iter().flatten().enumerate() {
                            let Ok(address) =
                                serde_json::from_value::<ObjectAddress>(item["address"].clone())
                            else {
                                continue;
                            };
                            let kind = item["kind"].as_str().unwrap_or("");
                            ui.push_id(i, |ui| {
                                ui.group(|ui| {
                                    ui.label(format!(
                                        "{} · {}구역 {}문단",
                                        match kind {
                                            "equation" => "수식",
                                            "footnote" => "각주",
                                            "endnote" => "미주",
                                            "textbox" => "글상자",
                                            "picture" => "그림",
                                            "table" => "표",
                                            _ => "도형",
                                        },
                                        address.section + 1,
                                        address.paragraph + 1
                                    ));
                                    ui.label(item["label"].as_str().unwrap_or(""));
                                    ui.horizontal(|ui| {
                                        if matches!(
                                            kind,
                                            "picture" | "shape" | "textbox" | "equation"
                                        ) && ui.button("속성").clicked()
                                        {
                                            self.open_object_properties(address);
                                            close = true;
                                        }
                                        if !matches!(kind, "shape" | "picture" | "table")
                                            && ui.button("내용 고치기").clicked()
                                        {
                                            let next =
                                                match kind {
                                                    "equation" => Ok(ObjectDialog::Equation {
                                                        address: Some(address),
                                                    }),
                                                    "textbox" => self
                                                        .session
                                                        .textbox_paragraphs(address)
                                                        .map(|p| ObjectDialog::Textbox {
                                                            address,
                                                            text: p.join("\n"),
                                                        }),
                                                    _ => self.session.note_paragraphs(address).map(
                                                        |p| ObjectDialog::Note {
                                                            address: Some(address),
                                                            endnote: kind == "endnote",
                                                            text: p.join("\n"),
                                                        },
                                                    ),
                                                };
                                            if let Some(next) = self.result(next) {
                                                self.feature_dialog = Some(
                                                    feature_dialogs::FeatureDialog::Object(next),
                                                );
                                                close = true;
                                            }
                                        }
                                        if matches!(kind, "shape" | "textbox")
                                            && ui.button("크기").clicked()
                                        {
                                            let r = self.session.shape_size(address);
                                            if let Some((width, height)) = self.result(r) {
                                                self.feature_dialog =
                                                    Some(feature_dialogs::FeatureDialog::Object(
                                                        ObjectDialog::Resize {
                                                            address,
                                                            width,
                                                            height,
                                                        },
                                                    ));
                                                close = true;
                                            }
                                        }
                                        if ui.button("지우기").clicked() {
                                            close = self.run("object.delete", json!(address));
                                        }
                                    });
                                });
                            });
                            if close {
                                break;
                            }
                        }
                    });
            }
            ObjectDialog::Symbols { category, text } => {
                ui.heading("문자표");
                ui.separator();
                let sets = [
                    ("기호", "※☆★○●◎◇◆□■△▲▽▼♠♣♥♦☎☞☜✓"),
                    ("수학", "±×÷≠≈≤≥∞∑∏√∫∂∆∇∈∉∪∩⊂⊃∀∃"),
                    ("화살표", "←↑→↓↔↕↖↗↘↙⇐⇑⇒⇓⇔"),
                    ("괄호·번호", "①②③④⑤⑥⑦⑧⑨⑩㈀㈁㈂㈃「」『』【】〈〉"),
                ];
                ui.horizontal(|ui| {
                    for (i, (label, _)) in sets.iter().enumerate() {
                        ui.selectable_value(category, i, *label);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for c in sets[*category].1.chars() {
                        if ui
                            .add_sized([34.0, 32.0], egui::Button::new(c.to_string()))
                            .clicked()
                        {
                            text.push(c);
                        }
                    }
                });
                ui.separator();
                ui.label("넣을 문자");
                ui.text_edit_singleline(text);
                if ui
                    .add_enabled(!text.is_empty(), egui::Button::new("넣기"))
                    .clicked()
                {
                    close = self.run("text.insert", json!({"text":text}));
                }
            }
        }
        close
    }
}
fn size_fields(ui: &mut egui::Ui, width: &mut f64, height: &mut f64) {
    ui.horizontal(|ui| {
        ui.label("너비");
        ui.add(egui::DragValue::new(width).range(1.0..=300.0).suffix(" mm"));
        ui.label("높이");
        ui.add(
            egui::DragValue::new(height)
                .range(1.0..=300.0)
                .suffix(" mm"),
        );
    });
}
