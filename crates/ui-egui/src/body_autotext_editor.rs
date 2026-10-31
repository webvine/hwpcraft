//! Additive rich body-autotext dialog; the plain-text abbreviation UI is separate.
use super::*;
use serde_json::Value;

#[derive(Clone, Debug, Default)]
struct Draft {
    name: String,
    description: String,
    capture: Option<Capture>,
    selection_status: String,
    overwrite: bool,
}

#[derive(Clone, Debug)]
struct Capture {
    generation: u64,
    source: Value,
    paragraphs: usize,
    summary: String,
}

#[derive(Clone, Debug, Default)]
struct Entry {
    id: String,
    name: String,
    description: String,
    summary: String,
    paragraphs: usize,
    bytes: usize,
}

/// Kept separate from the existing Ctrl+F3/Alt+I plain-text store and dialog.
#[derive(Default)]
pub(crate) struct State {
    entries: Vec<Entry>,
    revision: Option<Value>,
    selected: Option<String>,
    draft: Draft,
    status: String,
    preview: Option<Value>,
    preview_attempt: Option<(String, Option<Value>, u64)>,
    texture: Option<(egui::TextureHandle, Vec2)>,
    raster_attempted: bool,
    #[cfg(test)]
    action_regions: std::collections::BTreeMap<&'static str, (Rect, Rect, bool)>,
}

type Call<'a> = dyn FnMut(&str, Value) -> anyhow::Result<Value> + 'a;

impl State {
    #[cfg(test)]
    fn record_action(&mut self, name: &'static str, ui: &egui::Ui, response: &egui::Response) {
        self.action_regions
            .insert(name, (response.rect, ui.clip_rect(), response.enabled()));
    }
    fn load_list(&mut self, value: &Value) -> anyhow::Result<()> {
        let list = value["entries"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("본문 상용구 목록을 읽지 못했습니다."))?;
        let revision = value
            .get("revision")
            .filter(|value| value.is_u64())
            .ok_or_else(|| anyhow::anyhow!("본문 상용구 목록 버전이 없습니다."))?
            .clone();
        let entries: Vec<Entry> = list
            .iter()
            .map(|e| Entry {
                id: e["id"].as_str().unwrap_or("").to_owned(),
                name: e["name"].as_str().unwrap_or("").to_owned(),
                description: e["description"].as_str().unwrap_or("").to_owned(),
                summary: e["summary"].as_str().unwrap_or("").to_owned(),
                paragraphs: e["paragraphs"].as_u64().unwrap_or(0) as usize,
                bytes: e["bytes"].as_u64().unwrap_or(0) as usize,
            })
            .collect();
        if entries.iter().any(|e| e.id.is_empty()) {
            anyhow::bail!("본문 상용구 식별자를 읽지 못했습니다.");
        }
        let old_index = self
            .selected
            .as_ref()
            .and_then(|id| self.entries.iter().position(|e| &e.id == id))
            .unwrap_or(0);
        let selected = self
            .selected
            .clone()
            .filter(|id| entries.iter().any(|e| &e.id == id))
            .or_else(|| {
                entries
                    .get(old_index.min(entries.len().saturating_sub(1)))
                    .map(|e| e.id.clone())
            });
        if selected != self.selected || self.revision.as_ref() != Some(&revision) {
            self.preview = None;
            self.preview_attempt = None;
            self.texture = None;
            self.raster_attempted = false;
        }
        self.entries = entries;
        self.selected = selected;
        self.revision = Some(revision);
        Ok(())
    }

    fn refresh(&mut self, call: &mut Call<'_>) -> anyhow::Result<()> {
        self.load_list(&call("tools.autotext.body.list", json!({}))?)
    }

    fn capture(&mut self, call: &mut Call<'_>) -> anyhow::Result<()> {
        let value = call("tools.autotext.body.source", json!({}))?;
        let generation = value["generation"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("등록할 문서 상태를 읽지 못했습니다."))?;
        let source = value
            .get("source")
            .filter(|s| s.is_object())
            .ok_or_else(|| anyhow::anyhow!("등록할 본문 문단을 읽지 못했습니다."))?
            .clone();
        let paragraphs = value["paragraphs"].as_u64().unwrap_or(0) as usize;
        if paragraphs == 0 {
            anyhow::bail!("등록할 완전한 본문 문단을 선택하세요.");
        }
        self.draft.capture = Some(Capture {
            generation,
            source,
            paragraphs,
            summary: value["summary"].as_str().unwrap_or("").to_owned(),
        });
        self.draft.selection_status.clear();
        Ok(())
    }

    fn selected_params(&self) -> anyhow::Result<Value> {
        let id = self
            .selected
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("본문 상용구를 고르세요."))?;
        let mut params = json!({"id":id});
        params["expected_store_revision"] = self
            .revision
            .clone()
            .ok_or_else(|| anyhow::anyhow!("목록을 새로 고친 뒤 실행하세요."))?;
        Ok(params)
    }

    fn load_preview(&mut self, generation: u64, call: &mut Call<'_>) -> anyhow::Result<()> {
        self.preview_attempt = self
            .selected
            .clone()
            .map(|id| (id, self.revision.clone(), generation));
        let mut params = self.selected_params()?;
        params["expected_generation"] = json!(generation);
        let value = call("tools.autotext.body.preview", params)?;
        if value["svg"]
            .as_str()
            .is_some_and(|svg| svg.len() > 4 * 1024 * 1024)
        {
            anyhow::bail!("미리 보기의 크기가 한도를 초과했습니다.");
        }
        self.preview = Some(value);
        self.texture = None;
        self.raster_attempted = false;
        Ok(())
    }

    fn register(&mut self, call: &mut Call<'_>) -> anyhow::Result<()> {
        if !self.draft.selection_status.is_empty() {
            anyhow::bail!("등록할 본문 선택을 다시 읽어 주세요.");
        }
        let capture = self
            .draft
            .capture
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("등록할 본문 선택을 다시 읽어 주세요."))?;
        if self.draft.name.trim().is_empty() {
            anyhow::bail!("이름을 입력하세요.");
        }
        let mut params = json!({"name":self.draft.name.trim(),"description":self.draft.description,
            "source":capture.source,"expected_generation":capture.generation,"overwrite":self.draft.overwrite});
        params["expected_store_revision"] = self
            .revision
            .clone()
            .ok_or_else(|| anyhow::anyhow!("목록을 새로 고친 뒤 등록하세요."))?;
        let added = call("tools.autotext.body.add", params)?;
        if let Some(id) = added["entry"]["id"].as_str() {
            self.selected = Some(id.to_owned());
        }
        if let Some(revision) = added.get("revision") {
            self.revision = Some(revision.clone());
        }
        self.preview = None;
        self.preview_attempt = None;
        self.texture = None;
        self.raster_attempted = false;
        self.refresh(call)?;
        self.status = "본문 상용구를 등록했습니다.".into();
        Ok(())
    }

    fn delete(&mut self, call: &mut Call<'_>) -> anyhow::Result<()> {
        let value = call("tools.autotext.body.delete", self.selected_params()?)?;
        if value.get("entries").is_some() {
            self.load_list(&value)?;
        } else {
            self.refresh(call)?;
        }
        self.preview = None;
        self.preview_attempt = None;
        self.texture = None;
        self.raster_attempted = false;
        self.status = "본문 상용구를 지웠습니다.".into();
        Ok(())
    }

    fn insert(&mut self, generation: u64, call: &mut Call<'_>) -> anyhow::Result<()> {
        let mut params = self.selected_params()?;
        params["expected_generation"] = json!(generation);
        call("tools.autotext.body.insert", params)?;
        self.status = "캐럿 자리에 상용구를 넣었습니다.".into();
        Ok(())
    }

    fn open(session: &mut Session) -> Self {
        let mut state = Self::default();
        let mut call = |command: &str, params| session.run(command, params);
        if let Err(error) = state.refresh(&mut call) {
            state.status = format!("{error:#}");
        }
        if let Err(error) = state.capture(&mut call) {
            state.draft.selection_status = format!("{error:#}");
        }
        state
    }

    fn prepare_texture(&mut self, ctx: &egui::Context, renderer: &Renderer) {
        if self.texture.is_some() || self.raster_attempted {
            return;
        }
        let Some(svg) = self
            .preview
            .as_ref()
            .and_then(|v| v["svg"].as_str())
            .filter(|svg| !svg.is_empty())
        else {
            return;
        };
        self.raster_attempted = true;
        match renderer.svg(svg, 0.5) {
            Ok(page) => {
                let size = Vec2::new(page.width as f32, page.height as f32);
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [page.width as usize, page.height as usize],
                    &page.rgba,
                );
                self.texture = Some((
                    ctx.load_texture("body-autotext-preview", image, egui::TextureOptions::LINEAR),
                    size,
                ));
            }
            Err(error) => self.status = format!("미리 보기를 표시하지 못했습니다: {error:#}"),
        }
    }

    fn show(
        &mut self,
        ctx: &egui::Context,
        session: &mut Session,
        renderer: &Renderer,
    ) -> (bool, bool) {
        let generation = session.generation;
        let preview_key = self
            .selected
            .clone()
            .map(|id| (id, self.revision.clone(), generation));
        if self.selected.is_some() && self.preview_attempt != preview_key {
            self.preview = None;
            self.texture = None;
            self.raster_attempted = false;
            if let Err(error) = self.load_preview(generation, &mut |command, params| {
                session.run(command, params)
            }) {
                self.status = format!("{error:#}");
            }
        }
        self.prepare_texture(ctx, renderer);
        let mut close = false;
        let (mut refresh, mut capture, mut register, mut delete, mut insert) =
            (false, false, false, false, false);
        #[cfg(test)]
        self.action_regions.clear();
        let can_insert = session.cursor.cell.is_none()
            && !session.has_selection()
            && (session.cursor.offset == 0 || session.cursor.offset == session.len());
        let mut selected = self.selected.clone();
        chrome::modal("body-autotext-editor").show(ctx,|ui| {
            ui.set_width(640.0_f32.min((ctx.content_rect().width()-48.0).max(300.0)));
            // Let the scroll viewport measure the form against the real screen
            // budget, rather than retaining Area's small initial size forever.
            ui.set_max_height((ctx.content_rect().height()-150.0).max(260.0));
            close=chrome::title(ui,"본문 상용구");
            egui::ScrollArea::vertical().id_salt("body-autotext-content").max_height((ctx.content_rect().height()-150.0).max(260.0)).show(ui,|ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(260.0);
                        ui.horizontal(|ui| {ui.strong("등록된 본문 상용구");refresh=ui.button("새로 고침").clicked();});
                        egui::ScrollArea::vertical().id_salt("body-autotext-list").max_height(230.0).show(ui,|ui| {
                            if self.entries.is_empty() {ui.weak("등록된 본문 상용구가 없습니다.");}
                            for entry in &self.entries {
                                let response=ui.selectable_label(selected.as_ref()==Some(&entry.id),&entry.name);
                                if response.clicked() {selected=Some(entry.id.clone());}
                                response.on_hover_text(format!("{}\n{}개 문단 · {}바이트",entry.description,entry.paragraphs,entry.bytes));
                            }
                        });
                    });
                    let band=if self.entries.is_empty() {90.0} else {240.0};
                    ui.add_sized(Vec2::new(ui.spacing().item_spacing.x,band),egui::Separator::default().vertical());
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width().max(240.0));
                        ui.strong("미리 보기");
                        if let Some(entry)=self.entries.iter().find(|e|Some(&e.id)==selected.as_ref()) {
                            ui.label(&entry.description);
                            ui.label(RichText::new(format!("{}개 문단 · {}바이트",entry.paragraphs,entry.bytes)).size(12.5).color(Color32::DARK_GRAY));
                            if selected==self.selected && let Some((texture,size))=&self.texture {
                                let width=ui.available_width().min(size.x).min(420.0);
                                let display=Vec2::new(width,size.y*width/size.x.max(1.0));
                                egui::ScrollArea::vertical().id_salt("body-autotext-svg").max_height(200.0).show(ui,|ui| {
                                    ui.image((texture.id(),display));
                                });
                            } else {ui.add(egui::Label::new(&entry.summary).wrap());}
                        } else {ui.weak("목록에서 상용구를 고르세요.");}
                    });
                });
                ui.horizontal(|ui| {
                    let insert_why=if self.selected.is_none() {"목록에서 넣을 상용구를 고르세요."} else {"선택을 풀고 본문 문단의 시작이나 끝에 커서를 놓으세요(표 셀 안에는 넣을 수 없습니다)."};
                    let response=ui.add_enabled(self.selected.is_some()&&can_insert,egui::Button::new("현재 본문에 넣기"));
                    #[cfg(test)] self.record_action("insert",ui,&response);
                    insert=response.on_hover_text("본문 문단의 시작 또는 끝에 커서를 놓으면 넣을 수 있습니다.").on_disabled_hover_text(insert_why).clicked();
                    let response=ui.add_enabled(self.selected.is_some(),egui::Button::new("선택 항목 지우기"));
                    #[cfg(test)] self.record_action("delete",ui,&response);
                    delete=response.on_disabled_hover_text("목록에서 지울 상용구를 고르세요.").clicked();
                });
                chrome::section(ui,"현재 문단 선택 등록");
                ui.horizontal(|ui| {ui.label("이름");ui.add(egui::TextEdit::singleline(&mut self.draft.name).id_salt("body-autotext-name").desired_width(250.0));});
                ui.horizontal(|ui| {ui.label("설명");ui.add(egui::TextEdit::singleline(&mut self.draft.description).id_salt("body-autotext-description").desired_width(420.0));});
                if let Some(source)=&self.draft.capture {
                    ui.label(format!("구역 {} · 문단 {}–{} · {}개 문단",source.source["section"].as_u64().unwrap_or(0)+1,source.source["start"].as_u64().unwrap_or(0)+1,source.source["end"].as_u64().unwrap_or(0),source.paragraphs));
                    ui.add(egui::Label::new(&source.summary).wrap());
                    if source.generation!=generation {ui.colored_label(Color32::DARK_RED,"문서가 바뀌었습니다. 입력은 유지됩니다. 등록할 선택을 다시 읽어 주세요.");}
                }
                if !self.draft.selection_status.is_empty() {ui.colored_label(Color32::DARK_RED,&self.draft.selection_status);}
                ui.horizontal(|ui| {
                    let response=ui.button("현재 선택 다시 읽기");
                    #[cfg(test)] self.record_action("capture",ui,&response);
                    capture=response.clicked();
                    ui.checkbox(&mut self.draft.overwrite,"같은 이름 덮어쓰기");
                    let register_why=if self.draft.capture.is_none() {"[현재 선택 다시 읽기]로 등록할 문단을 먼저 읽으세요."} else if !self.draft.selection_status.is_empty() {"읽은 선택을 등록할 수 없습니다. 아래 빨간 안내를 확인하세요."} else if self.revision.is_none() {"[새로 고침]으로 목록을 읽은 뒤 등록하세요."} else {"이름을 입력하세요."};
                    let response=ui.add_enabled(self.draft.capture.is_some()&&self.draft.selection_status.is_empty()&&self.revision.is_some()&&!self.draft.name.trim().is_empty(),egui::Button::new("등록"));
                    #[cfg(test)] self.record_action("register",ui,&response);
                    register=response.on_disabled_hover_text(register_why).clicked();
                });
                ui.label(RichText::new("선택이 없으면 현재 본문 문단 전체를 등록합니다. 서식·표·개체를 포함할 문단은 처음부터 끝까지 선택하세요.").size(12.5).color(Color32::GRAY));
                if !self.status.is_empty() {ui.colored_label(Color32::DARK_RED,&self.status);}
            });
            ui.separator();
            close|=ui.button("닫기").clicked();
        });
        close |= ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape));
        if selected != self.selected {
            self.selected = selected;
            self.preview = None;
            self.preview_attempt = None;
            self.texture = None;
            self.raster_attempted = false;
        }
        let mut call = |command: &str, params| session.run(command, params);
        let result = if refresh {
            self.refresh(&mut call)
        } else if capture {
            self.capture(&mut call)
        } else if register {
            self.register(&mut call)
        } else if delete {
            self.delete(&mut call)
        } else if insert {
            self.insert(generation, &mut call)
        } else {
            Ok(())
        };
        let inserted = insert && result.is_ok();
        if let Err(error) = result {
            self.status = format!("{error:#}");
            if capture {
                self.draft.selection_status = self.status.clone();
            }
        }
        (close, inserted)
    }
}

impl HwpApp {
    pub(crate) fn open_body_autotext(&mut self) {
        if self.ext.core.body_autotext.is_some() {
            return;
        }
        self.ext.core.body_autotext = Some(State::open(&mut self.session));
    }
    pub(crate) fn body_autotext_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.ext.core.body_autotext.take() else {
            return;
        };
        let (close, inserted) = state.show(ctx, &mut self.session, &self.renderer);
        if inserted {
            self.sync_formatting();
            self.follow_cursor();
        }
        if !close {
            self.ext.core.body_autotext = Some(state);
        } else {
            ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("document-canvas")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(revision: u64, ids: &[&str]) -> Value {
        json!({"revision":revision,"path":"own-test-only",
            "entries":ids.iter().map(|id|json!({"id":id,"name":format!("이름 {id}"),
                "description":"표와 글자 모양","summary":"본문 미리 보기","paragraphs":2,"bytes":1200})).collect::<Vec<_>>()})
    }

    fn source(generation: u64) -> Value {
        json!({"generation":generation,"source":{"section":1,"start":2,"end":4},
            "paragraphs":2,"summary":"캡처한 완전한 두 문단"})
    }

    #[test]
    fn stale_registration_keeps_the_captured_selection_and_all_user_input() -> anyhow::Result<()> {
        let mut state = State::default();
        state.load_list(&list(4, &["a", "b"]))?;
        state.selected = Some("b".into());
        state.capture(&mut |command, params| {
            assert_eq!(command, "tools.autotext.body.source");
            assert_eq!(params, json!({}));
            Ok(source(31))
        })?;
        state.draft.name = "등록 초안".into();
        state.draft.description = "입력은 지워지면 안 됨".into();
        state.refresh(&mut |_, _| Ok(list(5, &["b", "a"])))?;
        let mut requests = Vec::new();
        assert!(
            state
                .register(&mut |command, params| {
                    requests.push((command.to_owned(), params));
                    anyhow::bail!("문서가 바뀌었습니다")
                })
                .is_err()
        );
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0, "tools.autotext.body.add");
        assert_eq!(requests[0].1["expected_generation"], 31);
        assert_eq!(requests[0].1["expected_store_revision"], 5);
        assert_eq!(requests[0].1["source"], source(31)["source"]);
        assert_eq!(state.draft.capture.as_ref().unwrap().generation, 31);
        assert_eq!(state.draft.name, "등록 초안");
        assert_eq!(state.draft.description, "입력은 지워지면 안 됨");
        assert_eq!(state.selected.as_deref(), Some("b"));
        Ok(())
    }

    #[test]
    fn refresh_delete_and_overwrite_keep_ids_and_invalidate_the_old_preview() -> anyhow::Result<()>
    {
        let mut state = State::default();
        state.load_list(&list(1, &["a", "b", "c"]))?;
        state.selected = Some("b".into());
        state.refresh(&mut |_, _| Ok(list(2, &["c", "b", "a"])))?;
        assert_eq!(state.selected.as_deref(), Some("b"));
        assert!(
            state
                .delete(&mut |command, params| {
                    assert_eq!(command, "tools.autotext.body.delete");
                    assert_eq!(params, json!({"id":"b","expected_store_revision":2}));
                    anyhow::bail!("목록이 바뀌었습니다")
                })
                .is_err()
        );
        assert_eq!(state.selected.as_deref(), Some("b"));
        state.delete(&mut |_, _| Ok(list(3, &["c", "a"])))?;
        assert_eq!(state.selected.as_deref(), Some("a"));
        state.capture(&mut |_, _| Ok(source(8)))?;
        state.draft.name = "이름 a".into();
        state.draft.overwrite = true;
        state.preview = Some(json!({"svg":"old"}));
        state.preview_attempt = Some(("a".into(), Some(json!(3)), 8));
        state.register(&mut |command, params| {
            if command == "tools.autotext.body.add" {
                assert_eq!(params["overwrite"], true);
                assert_eq!(params["expected_store_revision"], 3);
                Ok(json!({"revision":4,"entry":{"id":"a"}}))
            } else {
                Ok(list(4, &["c", "a"]))
            }
        })?;
        assert_eq!(state.selected.as_deref(), Some("a"));
        assert!(state.preview.is_none());
        assert!(state.preview_attempt.is_none());
        let mut inserted = Value::Null;
        state.insert(9, &mut |command, params| {
            assert_eq!(command, "tools.autotext.body.insert");
            inserted = params;
            Ok(json!({}))
        })?;
        assert_eq!(
            inserted,
            json!({"id":"a","expected_store_revision":4,"expected_generation":9})
        );
        Ok(())
    }

    #[test]
    fn preview_uses_the_selected_revision_and_rejects_oversize_output() -> anyhow::Result<()> {
        let mut state = State::default();
        state.load_list(&list(1, &["a"]))?;
        let mut calls = 0;
        assert!(
            state
                .load_preview(17, &mut |command, params| {
                    calls += 1;
                    assert_eq!(command, "tools.autotext.body.preview");
                    assert_eq!(
                        params,
                        json!({"id":"a","expected_store_revision":1,"expected_generation":17})
                    );
                    Ok(json!({"svg":"x".repeat(4*1024*1024+1)}))
                })
                .is_err()
        );
        assert_eq!(calls, 1);
        assert!(state.preview.is_none());
        assert_eq!(
            state.preview_attempt,
            Some(("a".into(), Some(json!(1)), 17))
        );
        state.refresh(&mut |_, _| Ok(list(2, &["a"])))?;
        assert!(state.preview_attempt.is_none());
        Ok(())
    }

    #[test]
    fn egui_dialog_renders_preview_and_blocks_document_input_without_changing_it()
    -> anyhow::Result<()> {
        let context = egui::Context::default();
        let mut app = HwpApp::with_context(context.clone(), None, None, false)?;
        app.session.execute(Command::Insert("사용자 문서".into()))?;
        let before = format!("{:?}", app.session.core.document());
        let generation = app.session.generation;
        let mut state = State::default();
        state.load_list(&list(7, &["a"]))?;
        state.capture(&mut |_, _| Ok(source(generation)))?;
        state.draft.name = "지우지 않을 등록 초안".into();
        state.preview = Some(
            json!({"svg":r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80"><rect width="120" height="80" fill="#fff"/><text x="4" y="20">Preview</text></svg>"##,
            "paragraphs":2,"summary":"미리 보기","import":{"allowed":true}}),
        );
        state.preview_attempt = Some(("a".into(), Some(json!(7)), generation));
        app.ext.core.body_autotext = Some(state);
        assert!(app.ext_modal_open());
        assert!(app.ext_enabled("tools.autotext.body"));
        for _ in 0..3 {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 900.0))),
                    ..Default::default()
                },
                |ui| app.body_autotext_dialog(ui.ctx()),
            );
            assert!(!output.shapes.is_empty());
            output.textures_delta.clear();
        }
        let state = app.ext.core.body_autotext.as_ref().unwrap();
        assert!(state.texture.is_some());
        assert_actions_visible(state, context.content_rect())?;
        assert_eq!(state.draft.name, "지우지 않을 등록 초안");
        assert_eq!(app.session.generation, generation);
        assert_eq!(format!("{:?}", app.session.core.document()), before);
        assert!(app.ext_modal_open());
        Ok(())
    }

    fn assert_actions_visible(state: &State, screen: Rect) -> anyhow::Result<()> {
        for action in ["insert", "delete", "capture", "register"] {
            let (rect, clip, enabled) =
                state.action_regions.get(action).copied().ok_or_else(|| {
                    anyhow::anyhow!("missing actual widget response for {action}")
                })?;
            assert!(enabled, "{action} fixture must exercise an enabled button");
            assert!(
                screen.contains_rect(rect),
                "{action} is outside the viewport: widget={rect:?} screen={screen:?}"
            );
            let visible = rect.intersect(clip);
            assert!(
                visible.width() >= rect.width() - 0.5 && visible.height() >= rect.height() - 0.5,
                "{action} is clipped: widget={rect:?} clip={clip:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn actual_engine_preview_contract_and_actions_use_an_isolated_child() -> anyhow::Result<()> {
        use std::io::Read;
        use std::process::{Command as ProcessCommand, Stdio};
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../output/implementation/0.17/ui-autotext-contract")
            .join(format!("{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        let mut child = ProcessCommand::new(std::env::current_exe()?)
            .args([
                "--exact",
                "body_autotext_editor::tests::actual_engine_preview_contract_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("HWPCRAFT_UI_AUTOTEXT_CONTRACT_CHILD", "1")
            .env("HWPCRAFT_CONFIG_DIR", &root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        fn capture(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let mut chunk = [0; 4096];
                while let Ok(count) = pipe.read(&mut chunk) {
                    if count == 0 {
                        break;
                    }
                    let available = (2 * 1024 * 1024_usize).saturating_sub(bytes.len());
                    bytes.extend_from_slice(&chunk[..count.min(available)]);
                }
                bytes
            })
        }
        let out = capture(
            child
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("child stdout"))?,
        );
        let error = capture(
            child
                .stderr
                .take()
                .ok_or_else(|| anyhow::anyhow!("child stderr"))?,
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut timed_out = false;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if std::time::Instant::now() >= deadline {
                timed_out = true;
                child.kill()?;
                break child.wait()?;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let stdout = out
            .join()
            .map_err(|_| anyhow::anyhow!("child stdout reader panic"))?;
        let stderr = error
            .join()
            .map_err(|_| anyhow::anyhow!("child stderr reader panic"))?;
        std::fs::write(root.join("child.stdout.log"), &stdout)?;
        std::fs::write(root.join("child.stderr.log"), &stderr)?;
        assert!(
            !timed_out,
            "isolated preview/UI contract child exceeded 30 seconds"
        );
        assert!(
            status.success(),
            "actual engine contract child failed: {}\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        Ok(())
    }

    #[test]
    fn actual_engine_preview_contract_child() -> anyhow::Result<()> {
        if std::env::var_os("HWPCRAFT_UI_AUTOTEXT_CONTRACT_CHILD").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return Ok(());
        }
        let config = std::env::var_os("HWPCRAFT_CONFIG_DIR")
            .ok_or_else(|| anyhow::anyhow!("isolated settings root missing"))?;
        let config = std::path::PathBuf::from(config);
        assert!(hwpcraft_engine::set_config_dir(config.clone()));
        let mut source = Session::new()?;
        source.execute(Command::Insert("서식 미리 보기 계약 검증 😀".into()))?;
        source.run("select.all", json!({}))?;
        source.execute(Command::Character(json!({"bold":true})))?;
        source.run("select.clear", json!({}))?;
        source.run("tools.autotext.body.add",json!({"name":"실제 엔진 항목","description":"실제 서식 조각","expected_generation":source.generation,"expected_store_revision":0}))?;
        let context = egui::Context::default();
        let mut app = HwpApp::with_context(context.clone(), None, None, false)?;
        let mut state = State::open(&mut app.session);
        state.draft.name = "보존할 등록 입력".into();
        state.draft.description = "변경되지 않는 설명".into();
        let selected = state.selected.clone();
        let missing = app
            .session
            .run("tools.autotext.body.preview", state.selected_params()?)
            .err()
            .ok_or_else(|| anyhow::anyhow!("actual engine accepted missing expected_generation"))?;
        assert!(format!("{missing:#}").contains("expected_generation"));
        let generation = app.session.generation;
        let before = format!("{:?}", app.session.core.document());
        state.load_preview(generation, &mut |command, params| {
            app.session.run(command, params)
        })?;
        std::fs::write(
            config.join("actual-preview.json"),
            serde_json::to_vec_pretty(&state.preview)?,
        )?;
        std::fs::write(
            config.join("actual-preview.svg"),
            state
                .preview
                .as_ref()
                .and_then(|v| v["svg"].as_str())
                .unwrap_or(""),
        )?;
        let preview = state
            .preview
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("actual preview missing"))?;
        assert_eq!(preview["entry"]["summary"], "서식 미리 보기 계약 검증 😀");
        let svg = preview["svg"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("actual preview SVG missing"))?;
        // The real native SVG emits individual positioned glyph text nodes;
        // reconstruct their text instead of assuming one contiguous XML run.
        let glyphs = svg
            .lines()
            .filter_map(|line| line.strip_suffix("</text>"))
            .filter_map(|line| line.rsplit_once('>').map(|(_, text)| text))
            .collect::<String>();
        assert_eq!(
            glyphs
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>(),
            "서식미리보기계약검증😀"
        );
        assert!(
            svg.contains("font-weight=\"bold\""),
            "preview lost registered character formatting"
        );
        assert_eq!(app.session.generation, generation);
        assert_eq!(format!("{:?}", app.session.core.document()), before);
        app.ext.core.body_autotext = Some(state);
        for _ in 0..3 {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 900.0))),
                    ..Default::default()
                },
                |ui| app.body_autotext_dialog(ui.ctx()),
            );
            output.textures_delta.clear();
        }
        let state = app
            .ext
            .core
            .body_autotext
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("dialog unexpectedly closed"))?;
        assert!(state.texture.is_some());
        assert_actions_visible(state, context.content_rect())?;
        assert!(
            state.status.is_empty(),
            "actual preview failed: {}",
            state.status
        );
        assert_eq!(state.selected, selected);
        assert_eq!(state.draft.name, "보존할 등록 입력");
        assert_eq!(format!("{:?}", app.session.core.document()), before);
        app.session
            .execute(Command::Insert("문서 변경 후".into()))?;
        let changed = app.session.generation;
        assert_ne!(changed, generation);
        let mut state = app
            .ext
            .core
            .body_autotext
            .take()
            .ok_or_else(|| anyhow::anyhow!("dialog missing"))?;
        assert!(
            state
                .load_preview(generation, &mut |command, params| app
                    .session
                    .run(command, params))
                .is_err(),
            "actual engine must reject a stale destination generation"
        );
        app.ext.core.body_autotext = Some(state);
        for _ in 0..3 {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 900.0))),
                    ..Default::default()
                },
                |ui| app.body_autotext_dialog(ui.ctx()),
            );
            output.textures_delta.clear();
        }
        let state = app
            .ext
            .core
            .body_autotext
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("dialog missing after generation change"))?;
        assert_eq!(
            state.preview_attempt.as_ref().map(|key| key.2),
            Some(changed)
        );
        assert!(state.preview.is_some());
        assert!(
            state.status.is_empty(),
            "new generation preview failed: {}",
            state.status
        );
        assert_eq!(state.draft.name, "보존할 등록 입력");
        assert_eq!(state.draft.description, "변경되지 않는 설명");
        assert_eq!(state.selected, selected);
        assert_actions_visible(state, context.content_rect())?;
        Ok(())
    }
}
