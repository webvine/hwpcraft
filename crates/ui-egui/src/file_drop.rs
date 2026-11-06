//! Files dragged in from another program. While they are over the window a mark at
//! the pointer says what letting go does there: on the document, a picture goes in
//! at the shown insertion point; on the AI 편집 panel's input box, the files are
//! attached for the AI. A document file dropped on the window opens, as before.
use super::*;

/// Where dropped files go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DropTarget {
    Document,
    /// The AI panel's input box.
    AiPanel,
    /// Elsewhere (the ribbon, the rest of the AI panel): pictures are not put in.
    Nowhere,
}

#[derive(Default)]
pub(crate) struct FileDrop {
    /// Pages drawn in the last frame (page, screen rect): a picture dropped on one
    /// goes in at the point it was let go.
    pub(crate) pages: Vec<(u32, Rect)>,
    /// Files dropped with the AI panel open where the pointer could not be told:
    /// the question of where they go.
    choice: Option<Vec<PathBuf>>,
}

const AI_PANEL_ID: &str = "ai-panel";
/// The AI panel's input box (`ai_panel::view`).
const AI_COMPOSER_ID: &str = "ai-panel-composer";

impl HwpApp {
    /// The pointer in window points while files are dragged over the window. winit
    /// 0.30 reports none then, so it is asked from Windows; `None` elsewhere.
    fn drag_pointer(ctx: &egui::Context) -> Option<Pos2> {
        let (x, y) = hwpcraft_render::cursor_screen_position()?;
        let inner = ctx.input(|i| i.viewport().inner_rect)?;
        let ppp = ctx.pixels_per_point();
        let p = Pos2::new(x as f32 / ppp, y as f32 / ppp) - inner.min.to_vec2();
        ctx.content_rect().contains(p).then_some(p)
    }

    /// The AI panel and its input box, when the panel is open.
    fn ai_panel_rects(&self, ctx: &egui::Context) -> Option<(Rect, Option<Rect>)> {
        if !self.ext.shell.settings.ai_panel_open {
            return None;
        }
        let load = |id: &str| {
            egui::containers::panel::PanelState::load(ctx, egui::Id::new(id)).map(|p| p.outer_rect)
        };
        Some((load(AI_PANEL_ID)?, load(AI_COMPOSER_ID)))
    }

    /// The AI panel's input box or the document view under `pointer`, else
    /// [`DropTarget::Nowhere`]; `None` when the AI panel is open and the pointer is
    /// not known (the document when it is closed).
    pub(crate) fn drop_target(
        &self,
        ctx: &egui::Context,
        pointer: Option<Pos2>,
    ) -> Option<DropTarget> {
        let panel = self.ai_panel_rects(ctx);
        let Some(p) = pointer else {
            return panel.is_none().then_some(DropTarget::Document);
        };
        if let Some((panel, composer)) = panel
            && panel.contains(p)
        {
            return Some(if composer.is_some_and(|c| c.contains(p)) {
                DropTarget::AiPanel
            } else {
                DropTarget::Nowhere
            });
        }
        Some(if self.wheel_zoom.viewport.is_none_or(|v| v.contains(p)) {
            DropTarget::Document
        } else {
            DropTarget::Nowhere
        })
    }

    pub(crate) fn drop_files(&mut self, files: &[PathBuf], ctx: &egui::Context) {
        let pointer = Self::drag_pointer(ctx);
        self.drop_files_at(files, pointer, ctx);
    }

    /// [`Self::drop_files`] let go at `pointer` (window points).
    pub(crate) fn drop_files_at(
        &mut self,
        files: &[PathBuf],
        pointer: Option<Pos2>,
        ctx: &egui::Context,
    ) {
        match self.drop_target(ctx, pointer) {
            Some(DropTarget::Document) => self.drop_in_document(files, pointer, ctx),
            Some(DropTarget::AiPanel) => self.drop_in_ai_panel(files),
            // A document still opens wherever it is let go.
            Some(DropTarget::Nowhere) if !Self::all_pictures(files) => {
                self.drop_in_document(files, None, ctx)
            }
            Some(DropTarget::Nowhere) => {
                self.message = "그림은 문서 위나 AI 편집 패널의 입력 칸에 놓으세요.".into();
            }
            None => self.file_drop.choice = Some(files.to_vec()),
        }
    }

    fn all_pictures(files: &[PathBuf]) -> bool {
        !files.is_empty() && files.iter().all(|f| clipboard::is_picture_file(f))
    }

    fn drop_in_ai_panel(&mut self, files: &[PathBuf]) {
        // The panel takes no attachments while a question runs.
        if !self.ai_take_dropped(files) {
            self.message = ext_shell::AI_LOCKED.into();
        }
    }

    /// Pictures (PNG, JPG, BMP, GIF) go in like 그림 넣기, one after another, at
    /// `pointer` when it is on a page, else at the caret; any other file opens as a
    /// document.
    fn drop_in_document(&mut self, files: &[PathBuf], pointer: Option<Pos2>, ctx: &egui::Context) {
        if !Self::all_pictures(files) {
            if let Some(path) = files.iter().find(|f| !clipboard::is_picture_file(f)) {
                self.request(Pending::Open(path.clone()), ctx);
            }
            return;
        }
        if self.ai_locked() {
            self.message = ext_shell::AI_LOCKED.into();
            return;
        }
        // A header, footer or nested text being edited would take the picture in the
        // body behind it.
        if self.session.hf_target().is_some() || self.session.nested_caret().is_some() {
            self.message = "머리말·꼬리말이나 표 안 글상자를 고치는 중에는 그림을 끌어 놓을 수 없습니다. 본문으로 돌아와 놓으세요.".into();
            return;
        }
        if let Some((page, c)) = pointer.and_then(|p| self.drop_point(p)) {
            let r = self.session.select(c, false);
            if self.result(r).is_some() {
                self.page = page;
            }
        }
        let mut placed = 0;
        for path in files {
            if !self.run("picture.insert", json!({"path": path})) {
                break;
            }
            placed += 1;
        }
        if placed > 0 {
            self.message = format!("그림 {placed}개를 넣었습니다.");
        }
    }

    /// The body position (page, cursor) under `p` when it is on a shown page; `None`
    /// on the grey around the pages or on a nested table, where the caret decides.
    fn drop_point(&self, p: Pos2) -> Option<(u32, Cursor)> {
        if self.wheel_zoom.viewport.is_some_and(|v| !v.contains(p)) {
            return None;
        }
        let &(page, rect) = self.file_drop.pages.iter().find(|(_, r)| r.contains(p))?;
        let pos = (p - rect.min) / self.zoom;
        let c = self
            .session
            .hit_test(page, pos.x as f64, pos.y as f64)
            .ok()?;
        Some((page, c))
    }

    /// Where a picture let go at `p` would go: the insertion bar on screen.
    fn drop_insertion_bar(&self, p: Pos2) -> Option<[Pos2; 2]> {
        let (_, c) = self.drop_point(p)?;
        let v = self.session.caret_at(c).ok()?;
        let page = v["pageIndex"].as_u64()? as u32;
        let &(_, rect) = self.file_drop.pages.iter().find(|(i, _)| *i == page)?;
        let n = |k: &str| v[k].as_f64().map(|x| x as f32 * self.zoom);
        let top = rect.min + Vec2::new(n("x")?, n("y")?);
        Some([top, top + Vec2::new(0.0, n("height")?.max(8.0))])
    }

    /// While files are dragged over the window: a mark at the pointer with what
    /// letting go there does, with the insertion point on the document or the AI
    /// panel's input box outlined; then the question for a drop whose place was not
    /// known.
    pub(crate) fn file_drop_ui(&mut self, ctx: &egui::Context) {
        self.drop_choice_dialog(ctx);
        let (any, files) = ctx.input(|i| {
            let files: Vec<PathBuf> = i
                .raw
                .hovered_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect();
            (!i.raw.hovered_files.is_empty(), files)
        });
        if !any {
            return;
        }
        // Windows sends nothing while the files move over the window.
        ctx.request_repaint_after(Duration::from_millis(30));
        let pointer = Self::drag_pointer(ctx);
        let pictures = Self::all_pictures(&files);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("file-drop"),
        ));
        let accent = chrome::Tokens::ACCENT;
        let (label, color) = match self.drop_target(ctx, pointer) {
            Some(DropTarget::Document) if pictures => {
                if let Some(bar) = pointer.and_then(|p| self.drop_insertion_bar(p)) {
                    painter.line_segment(bar, Stroke::new(2.5, accent));
                }
                ("문서에 그림 넣기", accent)
            }
            Some(DropTarget::Document | DropTarget::Nowhere) if !pictures => ("문서 열기", accent),
            Some(DropTarget::AiPanel) => {
                if let Some((_, Some(composer))) = self.ai_panel_rects(ctx) {
                    let frame = composer.shrink(2.0);
                    painter.rect_filled(
                        frame,
                        8.0,
                        Color32::from_rgba_unmultiplied(35, 103, 255, 30),
                    );
                    painter.rect_stroke(
                        frame,
                        8.0,
                        Stroke::new(2.0, accent),
                        egui::StrokeKind::Inside,
                    );
                }
                ("AI 편집에 첨부", accent)
            }
            Some(_) => ("여기에는 놓을 수 없습니다", Color32::from_gray(110)),
            None => ("놓은 뒤 넣을 곳을 고릅니다", accent),
        };
        let content = ctx.content_rect();
        // The mark: a ring at the pointer with the label beside it, like a tooltip.
        let at = pointer.unwrap_or(content.center());
        if pointer.is_some() {
            painter.circle(
                at,
                6.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 160),
                Stroke::new(2.0, color),
            );
        }
        let galley = painter.layout_no_wrap(
            label.into(),
            egui::FontId::proportional(13.0),
            Color32::WHITE,
        );
        let size = galley.size() + Vec2::new(20.0, 10.0);
        let mut pill = Rect::from_min_size(at + Vec2::new(14.0, 14.0), size);
        // Kept inside the window: to the pointer's left or above it near an edge.
        if pill.right() > content.right() - 4.0 {
            pill = pill.translate(Vec2::new(-(size.x + 28.0), 0.0));
        }
        if pill.bottom() > content.bottom() - 4.0 {
            pill = pill.translate(Vec2::new(0.0, -(size.y + 28.0)));
        }
        painter.rect_filled(pill, size.y / 2.0, color);
        painter.galley(pill.center() - galley.size() / 2.0, galley, Color32::WHITE);
    }

    /// Files dropped with the AI panel open where the pointer was not known (not
    /// Windows): the document or the panel.
    fn drop_choice_dialog(&mut self, ctx: &egui::Context) {
        let Some(files) = self.file_drop.choice.take() else {
            return;
        };
        let pictures = Self::all_pictures(&files);
        let (mut pick, mut close) = (None, false);
        let r = chrome::modal("file-drop-choice").show(ctx, |ui| {
            ui.set_width(380.0);
            close = chrome::title(ui, "끌어 놓은 파일");
            let names: Vec<String> = files
                .iter()
                .take(3)
                .filter_map(|f| f.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect();
            let more = files.len().saturating_sub(names.len());
            ui.label(if more > 0 {
                format!("{} 외 {more}개", names.join(", "))
            } else {
                names.join(", ")
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let document = if pictures {
                    "문서에 넣기"
                } else {
                    "문서 열기"
                };
                if ui.button(document).clicked() {
                    pick = Some(DropTarget::Document);
                }
                if ui.button("AI 편집 패널에 첨부").clicked() {
                    pick = Some(DropTarget::AiPanel);
                }
                close |= ui.button("취소").clicked();
            });
        });
        match pick {
            Some(DropTarget::Document) => self.drop_in_document(&files, None, ctx),
            Some(DropTarget::AiPanel) => self.drop_in_ai_panel(&files),
            Some(DropTarget::Nowhere) => {}
            None if !(close || r.should_close()) => self.file_drop.choice = Some(files),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paste_picture_tests::pictures;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hwpcraft-drop-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn picture(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        image::RgbImage::from_pixel(96, 48, image::Rgb([200, 40, 40]))
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        path
    }

    fn app() -> HwpApp {
        let mut app = HwpApp::with_context(egui::Context::default(), None, None, false).unwrap();
        app.ext.shell.settings.ai_panel_open = false;
        app
    }

    /// The AI panel open at the window's right, 300 points wide.
    fn open_ai_panel(app: &mut HwpApp) -> Rect {
        app.ext.shell.settings.ai_panel_open = true;
        let rect = Rect::from_min_max(Pos2::new(800.0, 60.0), Pos2::new(1100.0, 800.0));
        app.ctx.data_mut(|d| {
            d.insert_persisted(
                egui::Id::new(AI_PANEL_ID),
                egui::containers::panel::PanelState { outer_rect: rect },
            )
        });
        app.ctx.data_mut(|d| {
            d.insert_persisted(
                egui::Id::new(AI_COMPOSER_ID),
                egui::containers::panel::PanelState {
                    outer_rect: Rect::from_min_max(
                        Pos2::new(810.0, 640.0),
                        Pos2::new(1090.0, 790.0),
                    ),
                },
            )
        });
        app.wheel_zoom.viewport = Some(Rect::from_min_max(
            Pos2::new(0.0, 160.0),
            Pos2::new(790.0, 780.0),
        ));
        rect
    }

    #[test]
    fn dropped_pictures_go_in_at_the_caret() {
        let dir = scratch("pictures");
        let png = picture(&dir, "사진.PNG");
        let jpg = dir.join("photo.jpg");
        image::RgbImage::from_pixel(40, 40, image::Rgb([40, 40, 200]))
            .save(&jpg)
            .unwrap();
        let mut app = app();
        let ctx = app.ctx.clone();
        app.drop_files_at(&[png.clone(), jpg], None, &ctx);
        assert_eq!(pictures(&app), 2, "{}", app.message);
        assert!(app.error.is_none() && app.pending.is_none());
        assert!(app.session.dirty());
        // One Ctrl+Z takes the last picture out.
        app.command(Command::Undo);
        assert_eq!(pictures(&app), 1);

        // A broken picture says why and leaves the document as it was.
        let bad = dir.join("깨진.png");
        std::fs::write(&bad, b"not a picture").unwrap();
        app.drop_files_at(&[bad], None, &ctx);
        assert_eq!(pictures(&app), 1);
        assert!(app.error.as_deref().is_some_and(|e| e.contains("그림")));
        app.error = None;

        // Pictures stay out of a header being edited.
        let hit = app.session.hf_page_target(0, false).unwrap();
        assert!(app.hf_open(hit, 0, None));
        app.drop_files_at(&[png], None, &ctx);
        assert_eq!(pictures(&app), 1);
        assert!(app.message.contains("머리말"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A picture let go on a page goes in where it was let go, not at the caret.
    #[test]
    fn a_picture_goes_in_where_it_is_let_go() {
        let dir = scratch("point");
        let png = picture(&dir, "logo.png");
        let mut app = app();
        let s = &mut app.session;
        s.execute(Command::Insert("첫째 문단".into())).unwrap();
        s.execute(Command::Split).unwrap();
        s.execute(Command::Insert("둘째 문단".into())).unwrap();
        // Where the first paragraph's text starts, on page 0 drawn at (40, 50).
        let first = s
            .caret_at(hwpcraft_engine::Cursor {
                section: 0,
                paragraph: 0,
                offset: 2,
                cell: None,
            })
            .unwrap();
        let page = Rect::from_min_size(Pos2::new(40.0, 50.0), Vec2::new(794.0, 1123.0));
        app.file_drop.pages = vec![(0, page)];
        app.wheel_zoom.viewport = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 800.0)));
        let at = page.min
            + Vec2::new(
                first["x"].as_f64().unwrap() as f32,
                (first["y"].as_f64().unwrap() + first["height"].as_f64().unwrap() / 2.0) as f32,
            ) * app.zoom;
        // While dragged there, the insertion bar shows that place.
        let [top, bottom] = app.drop_insertion_bar(at).unwrap();
        assert!((top.y..=bottom.y).contains(&at.y) && (top.x - at.x).abs() < 20.0);
        let ctx = app.ctx.clone();
        app.drop_files_at(std::slice::from_ref(&png), Some(at), &ctx);
        assert_eq!(pictures(&app), 1, "{}", app.message);
        let objects = app.session.objects();
        let address = &objects[0]["address"];
        assert_eq!(address["paragraph"], 0, "{objects}");

        // On the grey beside the page the caret decides.
        let end = app.session.cursor;
        app.drop_files_at(&[png], Some(Pos2::new(10.0, 300.0)), &ctx);
        assert_eq!(pictures(&app), 2);
        assert_eq!(app.session.cursor.paragraph, end.paragraph);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_ai_panel_or_the_document_by_the_pointer() {
        let dir = scratch("target");
        let png = picture(&dir, "chart.png");
        let mut app = app();
        let ctx = app.ctx.clone();
        assert_eq!(app.drop_target(&ctx, None), Some(DropTarget::Document));
        let panel = open_ai_panel(&mut app);
        let composer = Pos2::new(900.0, 700.0);
        assert_eq!(
            app.drop_target(&ctx, Some(composer)),
            Some(DropTarget::AiPanel)
        );
        // The rest of the panel and the ribbon take no picture.
        assert_eq!(
            app.drop_target(&ctx, Some(panel.center_top() + Vec2::new(0.0, 40.0))),
            Some(DropTarget::Nowhere)
        );
        assert_eq!(
            app.drop_target(&ctx, Some(Pos2::new(300.0, 80.0))),
            Some(DropTarget::Nowhere)
        );
        app.drop_files_at(
            std::slice::from_ref(&png),
            Some(Pos2::new(300.0, 80.0)),
            &ctx,
        );
        assert_eq!(pictures(&app), 0);
        assert!(app.message.contains("입력 칸"), "{}", app.message);
        assert_eq!(
            app.drop_target(&ctx, Some(Pos2::new(300.0, 300.0))),
            Some(DropTarget::Document)
        );
        assert_eq!(app.drop_target(&ctx, None), None, "not known: asked");

        // On the input box the picture is not put in the document: the AI panel
        // attaches it to the next question.
        app.drop_files_at(std::slice::from_ref(&png), Some(composer), &ctx);
        assert_eq!(pictures(&app), 0);
        assert_eq!(app.ai_attachment_names(), ["chart.png"]);
        // Not known: the question, then 문서에 넣기 puts it in.
        app.drop_files_at(std::slice::from_ref(&png), None, &ctx);
        assert_eq!(
            app.file_drop.choice.as_deref(),
            Some(std::slice::from_ref(&png))
        );
        let choice = app.file_drop.choice.take().unwrap();
        app.drop_in_document(&choice, None, &ctx);
        assert_eq!(pictures(&app), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_dropped_document_still_opens() {
        let dir = scratch("document");
        let png = picture(&dir, "logo.png");
        let doc = dir.join("보고서.hwpx");
        let mut app = app();
        app.session.save(&doc).unwrap();
        let ctx = app.ctx.clone();
        app.drop_files_at(&[png, doc.clone()], None, &ctx);
        assert_eq!(
            pictures(&app),
            0,
            "a picture beside a document is not put in"
        );
        assert_eq!(app.session.path, Some(doc), "the document opens");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
