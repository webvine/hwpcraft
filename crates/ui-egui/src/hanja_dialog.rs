//! Owner: review-tools area. Hangul's 한자로 바꾸기 dialog (F9 / 입력 탭 한자 입력):
//! per-syllable candidate grid, 입력 형식, 바꾸기 / 한글로 바꾸기.
//! Other areas must not edit this file; request changes through the owner.
use super::*;
use serde_json::Value;

/// 입력 형식 list as the engine's `tools.hanja.convert` format ids.
const FORMATS: [(&str, &str); 4] = [
    ("hanja", "漢字"),
    ("hanja_hangul", "漢字(한글)"),
    ("hangul_hanja", "한글(漢字)"),
    ("hanja_bracket", "漢字[한글]"),
];
const COLUMNS: usize = 10;

pub(crate) struct Syllable {
    pub source: String,
    pub reading: String,
    /// (hanja, KS X 1001, 교육용, 인명용)
    pub candidates: Vec<(String, bool, bool, bool)>,
    pub chosen: Option<usize>,
}
pub(crate) struct HanjaDialog {
    pub text: String,
    pub hangul: String,
    pub items: Vec<Syllable>,
    pub current: usize,
    pub format: usize,
    /// Candidates the UI font cannot draw are dropped on the first frame.
    pub filtered: bool,
}
impl HanjaDialog {
    fn from_candidates(v: &Value) -> Option<Self> {
        let items: Vec<Syllable> = v["items"]
            .as_array()?
            .iter()
            .map(|item| Syllable {
                source: item["source"].as_str().unwrap_or("").to_owned(),
                reading: item["reading"]
                    .as_str()
                    .unwrap_or(item["source"].as_str().unwrap_or(""))
                    .to_owned(),
                candidates: item["candidates"]
                    .as_array()
                    .map(|c| {
                        c.iter()
                            .map(|x| {
                                (
                                    x["hanja"].as_str().unwrap_or("").to_owned(),
                                    x["ks_x_1001"].as_bool().unwrap_or(false),
                                    x["education"].as_bool().unwrap_or(false),
                                    x["personal_name"].as_bool().unwrap_or(false),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                chosen: None,
            })
            .collect();
        let current = items
            .iter()
            .position(|s| !s.candidates.is_empty())
            .unwrap_or(0);
        Some(Self {
            text: v["text"].as_str().unwrap_or("").to_owned(),
            hangul: v["hangul"].as_str().unwrap_or("").to_owned(),
            items,
            current,
            format: 0,
            filtered: false,
        })
    }
    /// Hanja string with the chosen candidate per syllable; unchosen syllables stay.
    fn assembled(&self) -> String {
        self.items
            .iter()
            .map(|s| match s.chosen {
                Some(i) => s.candidates[i].0.as_str(),
                None => s.source.as_str(),
            })
            .collect()
    }
    fn any_chosen(&self) -> bool {
        self.items.iter().any(|s| s.chosen.is_some())
    }
    /// Pick a candidate for the current syllable and advance to the next one with candidates.
    fn choose(&mut self, index: usize) {
        if let Some(s) = self.items.get_mut(self.current) {
            s.chosen = Some(index);
        }
        let next =
            (self.current + 1..self.items.len()).find(|i| !self.items[*i].candidates.is_empty());
        if let Some(n) = next {
            self.current = n;
        }
    }
}

impl HwpApp {
    /// Open 한자로 바꾸기 for the selection or the word at the caret.
    pub(crate) fn open_hanja_dialog(&mut self) {
        let r = self.session.run("tools.hanja.candidates", json!({}));
        let Some(v) = self.result(r) else {
            return;
        };
        let dialog = HanjaDialog::from_candidates(&v);
        match dialog {
            Some(d) if d.items.iter().any(|s| !s.candidates.is_empty()) => {
                self.ext.tools.hanja = Some(d);
            }
            _ => {
                self.message = format!(
                    "'{}'에 해당하는 한자가 없습니다.",
                    v["text"].as_str().unwrap_or("")
                );
            }
        }
    }

    pub(crate) fn hanja_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.tools.hanja.take() else {
            return;
        };
        if !d.filtered {
            // Hide hanja the UI font cannot draw (rare extension characters would show as boxes).
            let font = egui::FontId::proportional(20.0);
            ctx.fonts_mut(|f| {
                for s in &mut d.items {
                    s.candidates.retain(|c| f.has_glyphs(&font, &c.0));
                }
            });
            d.current = d
                .items
                .iter()
                .position(|s| !s.candidates.is_empty())
                .unwrap_or(0);
            d.filtered = true;
        }
        let mut action: Option<&str> = None;
        let mut keep = true;
        chrome::modal("hwp-hanja-dialog").show(ctx, |ui| {
            ui.set_width(560.0);
            if chrome::title(ui, "한자로 바꾸기") {
                keep = false;
            }
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(430.0);
                    egui::Grid::new("hanja-head")
                        .num_columns(2)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("한글");
                            ui.horizontal(|ui| {
                                for (i, s) in d.items.iter().enumerate() {
                                    let label = RichText::new(&s.source).size(16.0);
                                    let label = if i == d.current {
                                        label.color(chrome::Tokens::ACCENT).strong()
                                    } else {
                                        label
                                    };
                                    if ui
                                        .add(egui::Button::new(label).frame(false))
                                        .clicked()
                                        && !s.candidates.is_empty()
                                    {
                                        d.current = i;
                                    }
                                }
                            });
                            ui.end_row();
                            ui.label("한자");
                            ui.label(RichText::new(d.assembled()).size(16.0));
                            ui.end_row();
                        });
                    chrome::section(ui, "한자 목록");
                    let current = d.current;
                    let (reading, candidates): (String, Vec<(String, bool, bool, bool)>) = d
                        .items
                        .get(current)
                        .map(|s| (s.reading.clone(), s.candidates.clone()))
                        .unwrap_or_default();
                    let chosen = d.items.get(current).and_then(|s| s.chosen);
                    ui.label(
                        RichText::new(format!("음: {reading} ({}자)", candidates.len()))
                            .small()
                            .color(Color32::DARK_GRAY),
                    );
                    let mut pick: Option<usize> = None;
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui::Grid::new("hanja-candidates")
                                .num_columns(COLUMNS)
                                .spacing([4.0, 4.0])
                                .show(ui, |ui| {
                                    for (i, (hanja, ks, edu, name)) in
                                        candidates.iter().enumerate()
                                    {
                                        let text = RichText::new(hanja).size(20.0);
                                        let text = if !*ks {
                                            text.color(Color32::DARK_GRAY)
                                        } else {
                                            text
                                        };
                                        let r = ui.add(
                                            egui::Button::new(text)
                                                .selected(chosen == Some(i))
                                                .min_size(Vec2::new(36.0, 32.0)),
                                        );
                                        let mut hover = format!("{hanja} ({reading})");
                                        if *edu {
                                            hover.push_str(" · 교육용 기초 한자");
                                        }
                                        if *name {
                                            hover.push_str(" · 인명용 한자");
                                        }
                                        if !*ks {
                                            hover.push_str(" · KS X 1001 밖");
                                        }
                                        if r.on_hover_text(hover).clicked() {
                                            pick = Some(i);
                                        }
                                        if (i + 1) % COLUMNS == 0 {
                                            ui.end_row();
                                        }
                                    }
                                });
                        });
                    if let Some(i) = pick {
                        d.choose(i);
                    }
                    chrome::section(ui, "입력 형식");
                    ui.horizontal_wrapped(|ui| {
                        for (i, (_, label)) in FORMATS.iter().enumerate() {
                            ui.radio_value(&mut d.format, i, *label);
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "음절마다 한자를 고릅니다. 낱말 사전이 없어 낱말 단위 후보는 제공하지 않으며, 후보 순서는 KS X 1001·교육용 한자가 먼저입니다.",
                        )
                        .small()
                        .color(Color32::GRAY),
                    );
                });
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    let button = |ui: &mut egui::Ui, label: &str, enabled: bool| {
                        ui.add_enabled(
                            enabled,
                            egui::Button::new(label).min_size(Vec2::new(96.0, 26.0)),
                        )
                        .clicked()
                    };
                    if button(ui, "바꾸기(D)", d.any_chosen()) {
                        action = Some("convert");
                    }
                    if button(ui, "한글로 바꾸기", d.text.chars().any(|c| !('가'..='힣').contains(&c) && !c.is_ascii())) {
                        action = Some("to_hangul");
                    }
                    if button(ui, "닫기", true) {
                        keep = false;
                    }
                });
            });
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                keep = false;
            }
            if ui.input(|i| i.key_pressed(Key::Enter)) && d.any_chosen() {
                action = Some("convert");
            }
        });
        if action == Some("convert") {
            let hanja = d.assembled();
            let format = FORMATS[d.format].0;
            if self.run(
                "tools.hanja.convert",
                json!({"hanja": hanja, "format": format, "hangul": d.hangul}),
            ) {
                self.message = format!("'{}'을(를) 한자로 바꿨습니다.", d.text);
                keep = false;
            }
        } else if action == Some("to_hangul") && self.run("tools.hanja.to_hangul", json!({})) {
            self.message = format!("'{}'을(를) 한글로 바꿨습니다.", d.text);
            keep = false;
        }
        if keep {
            self.ext.tools.hanja = Some(d);
        } else {
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("document-canvas")));
        }
    }
}
