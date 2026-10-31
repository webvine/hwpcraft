//! Owner: format area. 덧말 넣기 window: annotation text, position, alignment and
//! size for the selected text, applied by the engine's `format.ruby`.
use super::super::*;

const POSITIONS: [(&str, &str); 2] = [("top", "위"), ("bottom", "아래")];
const ALIGNS: [(&str, &str); 3] = [("center", "가운데"), ("left", "왼쪽"), ("right", "오른쪽")];

pub(crate) struct RubyDialog {
    base: String,
    text: String,
    position: usize,
    align: usize,
    size: u8,
    blocked: Option<&'static str>,
}

impl HwpApp {
    pub(crate) fn open_ruby(&mut self) {
        self.ext.format.ruby = Some(RubyDialog {
            base: self.session.selected_text(),
            text: String::new(),
            position: 0,
            align: 0,
            size: 50,
            blocked: None,
        });
    }

    pub(crate) fn ruby_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.format.ruby.take() else {
            return;
        };
        let clicked = super::super::ext_shell::dialog(
            ctx,
            "덧말 넣기",
            440.0,
            &["넣기(D)", "취소"],
            |ui| {
                ui.horizontal(|ui| {
                    ui.label("본말");
                    ui.label(if d.base.is_empty() {
                        egui::RichText::new("(글자를 먼저 선택하세요)").weak()
                    } else {
                        egui::RichText::new(&d.base).strong()
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("덧말");
                    ui.add(egui::TextEdit::singleline(&mut d.text).desired_width(240.0));
                });
                super::super::ext_shell::group(ui, "모양");
                ui.horizontal(|ui| {
                    ui.label("위치");
                    for (i, (_, label)) in POSITIONS.iter().enumerate() {
                        ui.radio_value(&mut d.position, i, *label);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("정렬");
                    for (i, (_, label)) in ALIGNS.iter().enumerate() {
                        ui.radio_value(&mut d.align, i, *label);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("크기 비율");
                    ui.add(
                        egui::DragValue::new(&mut d.size)
                            .range(10..=100)
                            .suffix(" %"),
                    );
                });
                if let Some(why) = d.blocked {
                    ui.add_space(4.0);
                    ui.colored_label(egui::Color32::from_rgb(176, 0, 32), why);
                }
            },
        );
        match clicked {
            Some(0) => {
                let why = if d.base.is_empty() {
                    Some("본문 한 문단 안에서 덧말을 달 글자를 먼저 선택하세요.")
                } else if d.text.trim().is_empty() {
                    Some("덧말을 입력하세요.")
                } else {
                    None
                };
                if why.is_some() {
                    d.blocked = why;
                    self.ext.format.ruby = Some(d);
                    return;
                }
                self.run(
                    "format.ruby",
                    json!({"text": d.text.trim(), "position": POSITIONS[d.position].0,
                        "align": ALIGNS[d.align].0, "size": d.size}),
                );
            }
            Some(_) => {}
            None => self.ext.format.ruby = Some(d),
        }
    }
}
