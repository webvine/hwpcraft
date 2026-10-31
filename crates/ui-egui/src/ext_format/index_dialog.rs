//! Owner: format area. 색인 window (도구 > 색인): put an index mark after the selected
//! word through the engine's `index.mark.insert`, and make the index at the caret
//! with `tools.index`.
use super::super::*;

pub(crate) struct IndexDialog {
    first: String,
    second: String,
    /// Index marks in the document.
    marks: usize,
}

impl HwpApp {
    pub(crate) fn open_index(&mut self) {
        let first = if self.session.has_selection() && self.session.span_anchor.is_none() {
            self.session.selected_text().trim().to_owned()
        } else {
            String::new()
        };
        self.ext.format.index = Some(IndexDialog {
            first,
            second: String::new(),
            marks: self.index_mark_count(),
        });
    }

    fn index_mark_count(&mut self) -> usize {
        self.session
            .run("index.mark.list", json!({}))
            .ok()
            .and_then(|v| v["marks"].as_array().map(Vec::len))
            .unwrap_or(0)
    }

    pub(crate) fn index_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.ext.format.index.take() else {
            return;
        };
        let in_cell = self.session.cursor.cell.is_some() || self.session.span_anchor.is_some();
        let mark_grey = if in_cell {
            Some("색인 표시는 본문 한 문단 안에 넣습니다. 표 밖에 캐럿을 두세요.")
        } else if d.first.trim().is_empty() {
            Some("첫째 키를 쓰세요(낱말을 선택하고 열면 그 낱말이 들어갑니다).")
        } else {
            None
        };
        let make_grey = if self.session.cursor.cell.is_some() {
            Some("색인은 본문에 넣습니다. 표 밖에 캐럿을 두세요.")
        } else if d.marks == 0 {
            Some("문서에 색인 표시가 없습니다. 먼저 표시를 넣으세요.")
        } else {
            None
        };
        let clicked = super::super::ext_shell::dialog_with(
            ctx,
            "색인",
            460.0,
            &[
                ("색인 만들기(D)", make_grey),
                ("표시 넣기(M)", mark_grey),
                ("닫기", None),
            ],
            |ui| {
                super::super::ext_shell::group(ui, "색인 표시");
                egui::Grid::new("index-keys").num_columns(2).show(ui, |ui| {
                    ui.label("첫째 키");
                    let first =
                        ui.add(egui::TextEdit::singleline(&mut d.first).desired_width(240.0));
                    chrome::first_focus(&first);
                    ui.end_row();
                    ui.label("둘째 키");
                    ui.add(
                        egui::TextEdit::singleline(&mut d.second)
                            .desired_width(240.0)
                            .hint_text("없으면 비워 둡니다"),
                    );
                    ui.end_row();
                });
                ui.label(format!("문서의 색인 표시: {}개", d.marks));
                super::super::ext_shell::notice(
                    ui,
                    "표시 넣기는 선택한 낱말 뒤(선택이 없으면 캐럿)에 한글과 같은 색인 표시를 넣습니다. 색인 만들기는 캐럿 위치에 첫소리 묶음과 '낱말 쪽, 쪽' 줄로 색인을 넣습니다(한글은 새 문서에 만듭니다). 쪽 번호는 문서의 실제 쪽 순서입니다.",
                );
            },
        );
        match clicked {
            Some(0) => {
                self.run("tools.index", json!({}));
            }
            Some(1) => {
                let params = json!({"first": d.first.trim(), "second": d.second.trim()});
                if self.run("index.mark.insert", params) {
                    d.marks = self.index_mark_count();
                    d.first.clear();
                    d.second.clear();
                    self.message = format!("색인 표시를 넣었습니다. 문서에 {}개", d.marks);
                }
                self.ext.format.index = Some(d);
            }
            Some(_) => {}
            None => self.ext.format.index = Some(d),
        }
    }
}
