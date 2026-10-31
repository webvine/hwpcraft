//! Owner: format area. 차례 만들기 window (도구 > 제목 차례): collect outline
//! paragraphs or paragraphs in chosen styles, or captioned tables or pictures,
//! then insert a table through the engine's `tools.toc`, or refresh every table
//! of contents in the document with `tools.toc.update`.
use super::super::*;
use serde_json::Value;

const LEADERS: [(&str, &str); 4] = [
    ("dot", "점선"),
    ("dash", "파선"),
    ("solid", "실선"),
    ("none", "없음"),
];
const MAX_STYLE_LEVELS: usize = 5;
/// (engine `kind`, label, default title).
const KINDS: [(&str, &str, &str); 3] = [
    ("headings", "제목 차례", "차례"),
    ("tables", "표 차례", "표 차례"),
    ("pictures", "그림 차례", "그림 차례"),
];

pub(crate) struct TocDialog {
    /// Index into `KINDS`.
    kind: usize,
    by_styles: bool,
    levels: u8,
    /// Paragraph styles of the document: (id, name).
    styles: Vec<(usize, String)>,
    /// Chosen style per level, as an index into `styles`.
    picked: Vec<Option<usize>>,
    title: String,
    leader: usize,
    page_numbers: bool,
    numbers: bool,
    /// The document already has a table that 새로 고침 can rewrite.
    existing: bool,
    /// The document has outline paragraphs to collect.
    has_outline: bool,
    /// Why the last 만들기 was not carried out.
    blocked: Option<&'static str>,
}

impl TocDialog {
    fn params(&self) -> Value {
        let mut p = json!({
            "title": (!self.title.trim().is_empty()).then(|| self.title.trim().to_owned()),
            "leader": LEADERS[self.leader].0,
            "page_numbers": self.page_numbers,
        });
        if self.kind > 0 {
            p["kind"] = json!(KINDS[self.kind].0);
        } else if self.by_styles {
            let styles: Vec<Value> = self
                .picked
                .iter()
                .map_while(|pick| pick.map(|i| json!(self.styles[i].0)))
                .collect();
            p["styles"] = json!(styles);
        } else {
            p["levels"] = json!(self.levels);
            p["numbers"] = json!(self.numbers);
        }
        p
    }
}

impl HwpApp {
    /// 차례 만들기: outline paragraphs by default; style collection when the
    /// document has no outline paragraphs.
    pub(crate) fn open_toc(&mut self) {
        let styles: Vec<(usize, String)> = self
            .session
            .run("format.style.list", json!({}))
            .ok()
            .and_then(|v| v["items"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter(|s| s["type"] == "paragraph")
            .filter_map(|s| Some((s["id"].as_u64()? as usize, s["name"].as_str()?.to_owned())))
            .filter(|(_, name)| !name.starts_with("차례"))
            .collect();
        let outline = self
            .session
            .run("document.outline", json!({}))
            .ok()
            .is_some_and(|v| v["outline"].as_array().is_some_and(|a| !a.is_empty()));
        // A table of contents, or a 표/그림 차례, that 새로 고침 can rewrite.
        let existing = self
            .session
            .core
            .document()
            .doc_info
            .styles
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.local_name.starts_with("차례 ")
                    || s.local_name == "표 차례"
                    || s.local_name == "그림 차례"
            })
            .any(|(id, _)| {
                self.session
                    .core
                    .document()
                    .sections
                    .iter()
                    .flat_map(|s| s.paragraphs.iter())
                    .any(|p| usize::from(p.style_id) == id)
            });
        self.ext.format.toc = Some(TocDialog {
            kind: 0,
            by_styles: !outline,
            levels: 3,
            styles,
            picked: vec![None; MAX_STYLE_LEVELS],
            title: "차례".into(),
            leader: 0,
            page_numbers: true,
            numbers: true,
            existing,
            has_outline: outline,
            blocked: None,
        });
    }

    pub(crate) fn toc_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.format.toc.take() else {
            return;
        };
        let buttons: &[&str] = if d.existing {
            &["만들기(D)", "새로 고침(R)", "취소"]
        } else {
            &["만들기(D)", "취소"]
        };
        let clicked = super::super::ext_shell::dialog(
            ctx,
            "차례 만들기",
            520.0,
            buttons,
            |ui| {
                super::super::ext_shell::group(ui, "만들 차례");
                ui.horizontal(|ui| {
                    for (k, (_, label, title)) in KINDS.iter().enumerate() {
                        if ui.radio(d.kind == k, *label).clicked() && d.kind != k {
                            // Keep a title the user typed; swap only a default one.
                            if KINDS.iter().any(|(_, _, t)| d.title == *t) {
                                d.title = (*title).to_owned();
                            }
                            d.kind = k;
                            d.blocked = None;
                        }
                    }
                });
                if d.kind > 0 {
                    ui.label(if d.kind == 1 {
                        "캡션 달린 표의 캡션 글과 쪽을 모읍니다."
                    } else {
                        "캡션 달린 그림의 캡션 글과 쪽을 모읍니다."
                    });
                } else {
                    super::super::ext_shell::group(ui, "모을 대상");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut d.by_styles, false, "개요 문단");
                        ui.radio_value(&mut d.by_styles, true, "스타일");
                    });
                    if d.by_styles {
                        if !d.has_outline {
                            ui.label("개요 문단이 없어 스타일로 모읍니다. 제목에 쓰인 스타일을 수준마다 고르세요.");
                        }
                        for level in 0..MAX_STYLE_LEVELS {
                            // Levels are consecutive: a level shows once the one above is chosen.
                            if level > 0 && d.picked[level - 1].is_none() {
                                break;
                            }
                            ui.horizontal(|ui| {
                                ui.label(format!("{}수준", level + 1));
                                let shown =
                                    d.picked[level].map_or("(없음)", |i| d.styles[i].1.as_str());
                                egui::ComboBox::from_id_salt(("toc-style", level))
                                    .width(220.0)
                                    .selected_text(shown)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut d.picked[level], None, "(없음)");
                                        for (i, (_, name)) in d.styles.iter().enumerate() {
                                            ui.selectable_value(
                                                &mut d.picked[level],
                                                Some(i),
                                                name,
                                            );
                                        }
                                    });
                            });
                        }
                        for level in 1..MAX_STYLE_LEVELS {
                            if d.picked[level - 1].is_none() {
                                d.picked[level] = None;
                            }
                        }
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("개요 수준");
                            ui.add(egui::DragValue::new(&mut d.levels).range(1..=10));
                            ui.checkbox(&mut d.numbers, "개요 번호 넣기");
                        });
                    }
                }
                super::super::ext_shell::group(ui, "모양");
                ui.horizontal(|ui| {
                    ui.label("제목");
                    ui.add(egui::TextEdit::singleline(&mut d.title).desired_width(200.0));
                });
                ui.checkbox(&mut d.page_numbers, "쪽 번호 넣기");
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    ui.add_enabled_ui(d.page_numbers, |ui| {
                        ui.label("채움 선");
                        egui::ComboBox::from_id_salt("toc-leader")
                            .selected_text(LEADERS[d.leader].1)
                            .show_ui(ui, |ui| {
                                for (i, (_, label)) in LEADERS.iter().enumerate() {
                                    ui.selectable_value(&mut d.leader, i, *label);
                                }
                            });
                    });
                });
                if d.kind > 0 || (d.by_styles && d.picked[0].is_some()) {
                    d.blocked = None;
                }
                if let Some(why) = d.blocked {
                    ui.add_space(4.0);
                    ui.colored_label(egui::Color32::from_rgb(176, 0, 32), why);
                }
                super::super::ext_shell::notice(
                    ui,
                    "캐럿 위치에 차례를 일반 문단(차례 스타일)으로 넣습니다. 새로 고침은 문서의 제목·표·그림 차례를 한 번에 고칩니다. 쪽 번호는 문서의 실제 쪽 순서이며, 쪽 번호를 새로 시작한 구역은 인쇄된 번호와 다를 수 있습니다. 차례 항목 하이퍼링크와 한글 차례 필드는 아직 없습니다.",
                );
            },
        );
        match clicked {
            Some(0) => {
                if d.kind == 0 && d.by_styles && d.picked[0].is_none() {
                    // Keep the window open and say what is missing.
                    d.blocked = Some("1수준 스타일을 고르면 만들 수 있습니다.");
                    self.ext.format.toc = Some(d);
                    return;
                }
                self.run("tools.toc", d.params());
            }
            Some(1) if d.existing => {
                self.run("tools.toc.update", json!({}));
            }
            Some(_) => {}
            None => self.ext.format.toc = Some(d),
        }
    }
}
