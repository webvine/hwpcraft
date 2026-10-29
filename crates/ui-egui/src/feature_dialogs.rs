use super::ext_shell::{dialog, group, notice};
use super::*;
#[derive(Clone)]
pub(super) enum FeatureDialog {
    Object(super::object_dialogs::ObjectDialog),
    /// One-line header/footer with `[쪽 번호]`·`[전체 쪽수]`·`[파일 이름]` field tokens;
    /// `apply` is 0 both, 1 odd, 2 even.
    HeaderFooter {
        is_header: bool,
        apply: usize,
        text: String,
    },
    Link {
        uri: String,
        text: String,
    },
    Bookmarks {
        name: String,
    },
    Columns {
        count: u16,
        spacing: f64,
    },
    Search {
        query: String,
    },
}
impl HwpApp {
    pub(super) fn feature_dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.feature_dialog.take() else {
            return;
        };
        // Every dialog here uses the shared frame (title bar, right-hand buttons,
        // Enter = default, Esc = cancel); the object dialogs come from the
        // table-object area. The command search palette keeps its own layout.
        let keep = match &mut draft {
            FeatureDialog::HeaderFooter {
                is_header,
                apply,
                text,
            } => self.header_footer_text_dialog(ctx, *is_header, apply, text),
            FeatureDialog::Link { uri, text } => self.link_dialog(ctx, uri, text),
            FeatureDialog::Bookmarks { name } => self.bookmarks_dialog(ctx, name),
            FeatureDialog::Columns { count, spacing } => self.columns_dialog(ctx, count, spacing),
            FeatureDialog::Object(draft) => self.object_dialog_window(ctx, draft),
            FeatureDialog::Search { .. } => self.palette_dialog(ctx, &mut draft),
        };
        if keep && self.feature_dialog.is_none() {
            self.feature_dialog = Some(draft);
        }
    }

    /// 머리말/꼬리말 내용: one line with field tokens. Returns true to stay open.
    fn header_footer_text_dialog(
        &mut self,
        ctx: &egui::Context,
        is_header: bool,
        apply: &mut usize,
        text: &mut String,
    ) -> bool {
        let title = if is_header { "머리말" } else { "꼬리말" };
        let button = dialog(
            ctx,
            title,
            560.0,
            &["설정(D)", "이전", "다음", "취소"],
            |ui| {
                group(ui, "위치");
                ui.horizontal(|ui| {
                    for (i, label) in ["양쪽", "홀수 쪽", "짝수 쪽"].iter().enumerate() {
                        ui.radio_value(apply, i, *label);
                    }
                });
                group(ui, "내용");
                ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    ui.label("필드 넣기");
                    for (token, _) in super::ext_shell::HF_TOKENS {
                        if ui.button(token.trim_matches(['[', ']'])).clicked() {
                            text.push_str(token);
                        }
                    }
                    ui.add_enabled(false, egui::Button::new("날짜"))
                        .on_disabled_hover_text(
                            "RHWP 머리말/꼬리말에는 날짜 필드가 없습니다. 날짜는 글자로 입력하세요.",
                        );
                });
                notice(
                    ui,
                    "현재 구역에 적용하는 한 줄 내용입니다. 쪽 번호·전체 쪽수·파일 이름은 한글이 인쇄 때 채우는 필드로 저장됩니다.",
                );
            },
        );
        match button {
            Some(0) => {
                let (plain, fields) = super::ext_shell::split_hf_tokens(text);
                let fields: Vec<serde_json::Value> = fields
                    .iter()
                    .map(|(offset, kind)| json!({"offset": offset, "kind": kind}))
                    .collect();
                let apply_to = ["both", "odd", "even"][(*apply).min(2)];
                !self.run(
                    "page.header_footer.text",
                    json!({"header": is_header, "apply_to": apply_to, "text": plain, "fields": fields}),
                )
            }
            Some(1) | Some(2) => {
                self.step_header_footer(is_header, *apply, button == Some(2));
                false
            }
            Some(_) => false,
            None => true,
        }
    }

    fn link_dialog(&mut self, ctx: &egui::Context, uri: &mut String, text: &mut String) -> bool {
        let selected = (!self.session.selection().is_empty()).then(|| self.session.selected_text());
        let button = dialog(
            ctx,
            "하이퍼링크",
            520.0,
            &["넣기(D)", "취소"],
            |ui| {
                group(ui, "연결 대상");
                ui.label("웹 주소 (http / https)");
                ui.add(egui::TextEdit::singleline(uri).desired_width(f32::INFINITY));
                group(ui, "표시할 문자열");
                match &selected {
                    Some(selected) => {
                        ui.label(format!("선택한 문자열: {selected}"));
                    }
                    None => {
                        ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY));
                    }
                }
            },
        );
        match button {
            Some(0) => !self.run("insert.hyperlink", json!({"uri":uri,"text":text})),
            Some(_) => false,
            None => true,
        }
    }

    fn bookmarks_dialog(&mut self, ctx: &egui::Context, name: &mut String) -> bool {
        let in_body = self.session.cursor.cell.is_none();
        let list = self.session.bookmarks().ok();
        let mut action: Option<(&'static str, String)> = None;
        let button = dialog(ctx, "책갈피", 480.0, &["넣기(D)", "닫기"], |ui| {
            group(ui, "책갈피 이름");
            ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
            group(ui, "책갈피 목록");
            let names: Vec<String> = list
                .as_ref()
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter_map(|v| v["name"].as_str().map(str::to_owned))
                .collect();
            if names.is_empty() {
                ui.label(RichText::new("책갈피가 없습니다.").color(Color32::from_gray(120)));
            }
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    for label in names {
                        ui.push_id(&label, |ui| {
                            ui.horizontal(|ui| {
                                ui.add_sized([220.0, 24.0], egui::Label::new(&label).truncate());
                                if ui.button("이동").clicked() {
                                    action = Some(("bookmark.goto", label.clone()));
                                }
                                if ui.button("지우기").clicked() {
                                    action = Some(("bookmark.delete", label.clone()));
                                }
                            });
                        });
                    }
                });
            if !in_body {
                notice(ui, "책갈피는 표 밖 본문에서만 넣을 수 있습니다.");
            }
        });
        if let Some((command, label)) = action {
            let done = self.run(command, json!({"name": label}));
            // 이동 closes the dialog like Hangul; 지우기 keeps it open.
            return !(done && command == "bookmark.goto");
        }
        match button {
            Some(0) if in_body => {
                self.run("bookmark.add", json!({"name": name}));
                name.clear();
                true
            }
            Some(0) => true,
            Some(_) => false,
            None => true,
        }
    }

    fn columns_dialog(&mut self, ctx: &egui::Context, count: &mut u16, spacing: &mut f64) -> bool {
        let button = dialog(ctx, "다단 설정", 520.0, &["설정(D)", "취소"], |ui| {
            group(ui, "단 개수와 간격");
            ui.horizontal(|ui| {
                ui.label("단 수");
                ui.add(egui::DragValue::new(count).range(1..=6));
                ui.add_space(12.0);
                ui.label("단 사이");
                ui.add(
                    egui::DragValue::new(spacing)
                        .range(0.0..=50.0)
                        .suffix(" mm"),
                );
            });
            group(ui, "미리 보기");
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 96.0), egui::Sense::hover());
            for n in 0..*count {
                let width = rect.width() / f32::from(*count);
                let r = Rect::from_min_size(
                    rect.min + Vec2::new(f32::from(n) * width, 0.0),
                    Vec2::new(width - 8.0, rect.height()),
                );
                ui.painter().rect_filled(r, 0.0, Color32::from_gray(236));
            }
            notice(ui, "현재 구역에 같은 너비의 일반 다단을 적용합니다.");
        });
        match button {
            Some(0) => !self.run("page.columns", json!({"count":count,"spacing_mm":spacing})),
            Some(_) => false,
            None => true,
        }
    }

    /// The command search palette.
    fn palette_dialog(&mut self, ctx: &egui::Context, draft: &mut FeatureDialog) -> bool {
        let mut close = false;
        let response = chrome::modal("feature-dialog").show(ctx, |ui| {
            ui.set_width(470.0);
            match draft {
                FeatureDialog::Search { query } => {
                    ui.heading("명령 찾기");
                    ui.text_edit_singleline(query);
                    ui.separator();
                    close = self.command_search_results(ui, query);
                }
                _ => close = true,
            }
            ui.separator();
            if ui.button("닫기").clicked() {
                close = true;
            }
        });
        !close && !response.should_close()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(a: &mut HwpApp, events: Vec<egui::Event>) {
        let ctx = a.ctx.clone();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |_| a.feature_dialogs(&ctx),
        );
        output.textures_delta.clear();
    }

    fn enter() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn shared_frame_dialogs_apply_on_enter_and_close_on_escape() -> anyhow::Result<()> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        a.session.execute(Command::Insert("본문".into()))?;
        // Bookmarks: Enter adds the typed name and keeps the dialog open.
        a.feature_dialog = Some(FeatureDialog::Bookmarks {
            name: "첫째".into(),
        });
        frame(&mut a, vec![]);
        frame(&mut a, vec![enter()]);
        let names = a.session.bookmarks()?;
        assert!(names.to_string().contains("첫째"), "{names}");
        assert!(matches!(
            &a.feature_dialog,
            Some(FeatureDialog::Bookmarks { name }) if name.is_empty()
        ));
        // Esc closes without changing anything.
        let generation = a.session.generation;
        frame(
            &mut a,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(a.feature_dialog.is_none());
        assert_eq!(a.session.generation, generation);
        // Columns: Enter applies and closes.
        a.feature_dialog = Some(FeatureDialog::Columns {
            count: 2,
            spacing: 8.0,
        });
        frame(&mut a, vec![]);
        frame(&mut a, vec![enter()]);
        assert!(a.feature_dialog.is_none());
        assert!(a.session.can_undo());
        Ok(())
    }
}
