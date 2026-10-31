//! Owner: format area. 문단 첫 글자 장식 window (서식 > 문단 첫 글자 장식): shape
//! (없음·2줄·3줄·여백), letter font and gap, applied by the engine's `format.drop_cap`.
use super::super::*;

const SHAPES: [(&str, &str); 4] = [
    ("none", "없음"),
    ("double", "2줄"),
    ("triple", "3줄"),
    ("margin", "여백"),
];

pub(crate) struct DropCapDialog {
    shape: usize,
    font: String,
    gap_mm: f64,
}

impl HwpApp {
    pub(crate) fn open_drop_cap(&mut self) {
        self.ext.format.drop_cap = Some(DropCapDialog {
            shape: 1,
            font: String::new(),
            gap_mm: 1.0,
        });
    }

    pub(crate) fn drop_cap_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.format.drop_cap.take() else {
            return;
        };
        let clicked = super::super::ext_shell::dialog(
            ctx,
            "문단 첫 글자 장식",
            460.0,
            &["설정(D)", "취소"],
            |ui| {
                super::super::ext_shell::group(ui, "모양");
                ui.horizontal(|ui| {
                    for (i, (_, label)) in SHAPES.iter().enumerate() {
                        ui.radio_value(&mut d.shape, i, *label);
                    }
                });
                super::super::ext_shell::group(ui, "글자");
                ui.add_enabled_ui(d.shape != 0, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("글꼴");
                        ui.add(
                            egui::TextEdit::singleline(&mut d.font)
                                .hint_text("본문 글꼴")
                                .desired_width(180.0),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("본문과의 간격");
                        ui.add(
                            egui::DragValue::new(&mut d.gap_mm)
                                .range(0.0..=10.0)
                                .speed(0.1)
                                .suffix(" mm"),
                        );
                    });
                });
                super::super::ext_shell::notice(
                    ui,
                    "캐럿이 있는 문단의 첫 글자를 테두리 없는 글상자로 옮깁니다. 없음은 장식을 없애고 글자를 되돌립니다. 글은 장식 옆으로 흐릅니다. 한 문단에 글상자·그림처럼 둘레로 글이 흐르는 개체가 이미 있으면 HwpCraft 화면에서는 장식이 글과 겹쳐 보일 수 있습니다(한글에서는 정상).",
                );
            },
        );
        match clicked {
            Some(0) => {
                let mut params = json!({"style": SHAPES[d.shape].0});
                if d.shape != 0 {
                    params["gap_mm"] = json!(d.gap_mm);
                    if !d.font.trim().is_empty() {
                        params["font"] = json!(d.font.trim());
                    }
                }
                self.run("format.drop_cap", params);
            }
            Some(_) => {}
            None => self.ext.format.drop_cap = Some(d),
        }
    }
}
