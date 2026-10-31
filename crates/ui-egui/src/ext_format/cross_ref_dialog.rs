//! Owner: format area. 상호 참조 window (입력 > 상호 참조): pick a heading, a
//! captioned table or picture, or a bookmark and insert the page it is on through
//! the engine's `insert.cross_reference`; 모두 새로 고침 runs
//! `tools.cross_reference.update`.
use super::super::*;
use serde_json::Value;

/// What a table reference shows: (engine `show`, label).
const SHOWS: [(&str, &str); 3] = [
    ("page", "쪽 번호"),
    ("number", "캡션 번호"),
    ("caption", "캡션 전체"),
];
const KINDS: [(&str, &str); 4] = [
    ("headings", "개요"),
    ("tables", "표"),
    ("pictures", "그림"),
    ("bookmarks", "책갈피"),
];

pub(crate) struct CrossRefDialog {
    kind: usize,
    /// `tools.cross_reference.targets` when the window opened.
    targets: Value,
    picked: Option<usize>,
    /// Index into `SHOWS`; only tables offer more than the page.
    show: usize,
    /// Cross references already in the document, for 모두 새로 고침.
    references: usize,
}

impl CrossRefDialog {
    fn items(&self) -> &[Value] {
        self.targets[KINDS[self.kind].0]
            .as_array()
            .map_or(&[], Vec::as_slice)
    }

    /// The list line and the `target` parameter of one item.
    fn entry(&self, item: &Value) -> (String, Value) {
        let page = item["page"]
            .as_u64()
            .map_or_else(|| "쪽 없음".to_owned(), |p| format!("{p}쪽"));
        let text = |key: &str| item[key].as_str().unwrap_or_default().to_owned();
        match KINDS[self.kind].0 {
            "headings" => {
                let indent = "  ".repeat(item["level"].as_u64().unwrap_or(1) as usize - 1);
                (
                    format!("{indent}{}  ({page})", text("title")),
                    json!({"heading": text("title")}),
                )
            }
            "bookmarks" => (
                format!("{}  ({page})", text("name")),
                json!({"bookmark": text("name")}),
            ),
            kind => {
                let n = item["n"].as_u64().unwrap_or(0);
                let caption = text("caption");
                let shown = if caption.is_empty() {
                    format!("{} {n}", KINDS[self.kind].1)
                } else {
                    caption
                };
                let key = if kind == "tables" { "table" } else { "picture" };
                (format!("{shown}  ({page})"), json!({ key: n }))
            }
        }
    }
}

impl HwpApp {
    pub(crate) fn open_cross_ref(&mut self) {
        let targets = self
            .session
            .run("tools.cross_reference.targets", json!({}))
            .unwrap_or_default();
        let mut d = CrossRefDialog {
            kind: 0,
            targets,
            picked: None,
            show: 0,
            references: self.session.cross_references().len(),
        };
        // Open on the first kind the document has.
        d.kind = (0..KINDS.len())
            .find(|&k| {
                d.kind = k;
                !d.items().is_empty()
            })
            .unwrap_or(0);
        self.ext.format.cross_ref = Some(d);
    }

    pub(crate) fn cross_ref_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.format.cross_ref.take() else {
            return;
        };
        let blocked = self.session.cross_reference_blocked();
        let insert_grey = blocked.or_else(|| {
            d.picked
                .is_none()
                .then_some("가리킬 대상을 목록에서 고르세요.")
        });
        let refresh_grey = (d.references == 0).then_some("문서에 상호 참조가 없습니다.");
        let clicked = super::super::ext_shell::dialog_with(
            ctx,
            "상호 참조",
            520.0,
            &[
                ("넣기(D)", insert_grey),
                ("모두 새로 고침(R)", refresh_grey),
                ("닫기", None),
            ],
            |ui| {
                super::super::ext_shell::group(ui, "참조 대상");
                ui.horizontal(|ui| {
                    for (k, (_, label)) in KINDS.iter().enumerate() {
                        if ui.radio(d.kind == k, *label).clicked() && d.kind != k {
                            d.kind = k;
                            d.picked = None;
                            d.show = 0;
                        }
                    }
                });
                let entries: Vec<(String, Value)> =
                    d.items().iter().map(|item| d.entry(item)).collect();
                egui::Frame::NONE
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(190)))
                    .inner_margin(4.0)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("cross-ref-targets")
                            .max_height(220.0)
                            .min_scrolled_height(220.0)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                if entries.is_empty() {
                                    ui.label(match KINDS[d.kind].0 {
                                        "headings" => "개요 문단이 없습니다.",
                                        "bookmarks" => "책갈피가 없습니다.",
                                        _ => "캡션 달린 개체가 없습니다. 캡션을 넣으면 여기에 나옵니다.",
                                    });
                                }
                                for (i, (label, _)) in entries.iter().enumerate() {
                                    if ui
                                        .selectable_label(d.picked == Some(i), label)
                                        .clicked()
                                    {
                                        d.picked = Some(i);
                                    }
                                }
                            });
                    });
                super::super::ext_shell::group(ui, "참조 내용");
                if KINDS[d.kind].0 == "tables" {
                    ui.horizontal(|ui| {
                        for (i, (_, label)) in SHOWS.iter().enumerate() {
                            ui.radio_value(&mut d.show, i, *label);
                        }
                    });
                    if d.show == 1 {
                        ui.label("한글처럼 번호만 넣습니다. \"표 \"는 앞에 직접 입력하세요.");
                    }
                } else {
                    ui.label("쪽 번호");
                }
                if let Some(why) = blocked {
                    ui.add_space(4.0);
                    ui.colored_label(egui::Color32::from_rgb(176, 0, 32), why);
                }
                super::super::ext_shell::notice(
                    ui,
                    "캐럿 위치에 대상의 쪽 번호(표는 캡션 번호·캡션도)를 넣습니다. 쪽이나 캡션이 바뀌는 편집 뒤에는 모두 새로 고침을 누르세요. 쪽 번호는 문서의 실제 쪽 순서입니다. 그림·개요의 번호·제목 참조는 아직 없습니다.",
                );
            },
        );
        match clicked {
            Some(0) => {
                if let Some((_, target)) = d
                    .picked
                    .and_then(|i| d.items().get(i))
                    .map(|item| d.entry(item))
                {
                    let show = if KINDS[d.kind].0 == "tables" {
                        SHOWS[d.show].0
                    } else {
                        "page"
                    };
                    self.run(
                        "insert.cross_reference",
                        json!({ "target": target, "show": show }),
                    );
                }
            }
            Some(1) => {
                self.run("tools.cross_reference.update", json!({}));
            }
            Some(_) => {}
            None => self.ext.format.cross_ref = Some(d),
        }
    }
}
