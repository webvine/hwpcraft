//! Page rasterization off the UI thread. Making a page's SVG needs the document and
//! stays on the UI thread (5–10 ms); parsing and rasterizing it (40–50 ms per page of
//! the 381-page handbook) runs on workers, so scrolling to a page not seen yet and
//! zoom steps no longer stall a frame. Until its bitmap arrives a new page shows blank
//! paper, and a page at the old zoom step keeps its old bitmap, scaled.
//! Edits still redraw synchronously (band or whole page) so typing shows at once.
use super::*;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

/// Worker threads; each keeps its own renderer (fonts are shared process-wide).
const WORKERS: usize = 2;

/// What a page bitmap depends on besides the document: 그림 on/off, the view colours and
/// 한자 발음 표시.
pub(crate) type Style = (bool, Option<(Color32, Color32)>, bool);

/// Identity of a requested bitmap; a result is used only if it still matches.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Key {
    pub page: u32,
    pub generation: u64,
    pub scale: u16,
    pub style: Style,
}

struct Job {
    key: Key,
    scale: f32,
    svg: String,
}

pub(crate) struct Done {
    pub key: Key,
    pub scale: f32,
    pub image: Result<egui::ColorImage, String>,
    pub ms: f64,
}

pub(crate) struct Rasterizer {
    jobs: Sender<Job>,
    done: Receiver<Done>,
    /// Keys sent and not yet received.
    pub busy: Vec<Key>,
}

impl Rasterizer {
    pub fn new(ctx: &egui::Context) -> Self {
        let (jobs, receive) = channel::<Job>();
        let (send, done) = channel::<Done>();
        let receive = Arc::new(Mutex::new(receive));
        for n in 0..WORKERS {
            let receive = receive.clone();
            let send = send.clone();
            let ctx = ctx.clone();
            let _ = std::thread::Builder::new()
                .name(format!("hwpcraft-page-raster-{n}"))
                .spawn(move || {
                    let renderer = hwpcraft_render::Renderer::default();
                    loop {
                        let job = match receive.lock() {
                            Ok(r) => r.recv(),
                            Err(_) => return,
                        };
                        let Ok(job) = job else {
                            return;
                        };
                        let start = std::time::Instant::now();
                        let image = rasterize(&renderer, &job);
                        let ms = start.elapsed().as_secs_f64() * 1000.0;
                        if send
                            .send(Done {
                                key: job.key,
                                scale: job.scale,
                                image,
                                ms,
                            })
                            .is_err()
                        {
                            return;
                        }
                        ctx.request_repaint();
                    }
                });
        }
        Self {
            jobs,
            done,
            busy: Vec::new(),
        }
    }

    /// True when a worker is free for another page.
    pub fn idle(&self) -> bool {
        self.busy.len() < WORKERS
    }

    fn send(&mut self, key: Key, scale: f32, svg: String) {
        if self.jobs.send(Job { key, scale, svg }).is_ok() {
            self.busy.push(key);
        }
    }

    pub fn receive(&mut self) -> Vec<Done> {
        let done: Vec<Done> = self.done.try_iter().collect();
        self.busy.retain(|key| !done.iter().any(|d| d.key == *key));
        done
    }
}

fn rasterize(renderer: &hwpcraft_render::Renderer, job: &Job) -> Result<egui::ColorImage, String> {
    let (pictures, colors, readings) = job.key.style;
    let mut svg = std::borrow::Cow::Borrowed(job.svg.as_str());
    if !pictures {
        svg = std::borrow::Cow::Owned(picture_placeholders(&svg));
    }
    if readings {
        svg = std::borrow::Cow::Owned(super::hanja_view::hanja_readings_svg(&svg));
    }
    if let Some((paper, text)) = colors {
        svg = std::borrow::Cow::Owned(view_colors_svg(&svg, paper, text));
    }
    let page = renderer
        .screen(&svg, job.scale)
        .map_err(|e| e.to_string())?;
    Ok(egui::ColorImage::from_rgba_premultiplied(
        [page.width as usize, page.height as usize],
        &page.rgba,
    ))
}

impl HwpApp {
    /// Workers draw new pages, except while a `--screenshot` is pending: the capture
    /// is taken on a fixed frame and must show finished pages.
    pub(crate) fn raster_async(&self) -> bool {
        self.ext.shell.async_raster && self.screenshot.is_none()
    }

    pub(crate) fn page_style(&self) -> Style {
        (
            self.ext.shell.show_pictures,
            self.ext
                .shell
                .view_color
                .colors(self.ext.shell.custom_colors),
            self.ext.shell.hanja_readings,
        )
    }

    /// Ask a worker for `page` at `scale` unless it is already on its way. Makes at
    /// most one page SVG per pass so the UI thread's share stays small.
    pub(crate) fn queue_raster(&mut self, ctx: &egui::Context, page: u32, scale: f32) {
        let key = Key {
            page,
            generation: self.session.generation,
            scale: scale_key(scale),
            style: self.page_style(),
        };
        let pass = ctx.cumulative_pass_nr();
        let raster = self
            .ext
            .shell
            .raster
            .get_or_insert_with(|| Rasterizer::new(ctx));
        if raster.busy.contains(&key) {
            return;
        }
        if !raster.idle() || self.ext.shell.raster_svg_pass == Some(pass) {
            ctx.request_repaint();
            return;
        }
        self.ext.shell.raster_svg_pass = Some(pass);
        let t0 = std::time::Instant::now();
        match self.session.render_page_svg(page) {
            Ok(svg) => {
                let [a, b] = self.ext.shell.raster_ms.get();
                self.ext
                    .shell
                    .raster_ms
                    .set([a + t0.elapsed().as_secs_f64() * 1000.0, b]);
                if let Some(raster) = self.ext.shell.raster.as_mut() {
                    raster.send(key, scale, svg);
                }
            }
            Err(e) => self.message = format!("쪽 조판 실패: {e}"),
        }
    }

    /// Take finished bitmaps that still match the document, zoom step and colours.
    pub(crate) fn receive_rasters(&mut self, ctx: &egui::Context) {
        let Some(raster) = self.ext.shell.raster.as_mut() else {
            return;
        };
        let done = raster.receive();
        let generation = self.session.generation;
        let style = self.page_style();
        for Done {
            key,
            scale,
            image,
            ms,
        } in done
        {
            let [a, b] = self.ext.shell.raster_ms.get();
            self.ext.shell.raster_ms.set([a, b + ms]);
            if key.generation != generation || key.style != style {
                continue;
            }
            let wanted = scale_key(fit_texture(
                raster_scale(self.zoom, ctx.pixels_per_point()),
                self.page_sizes.1.get(key.page as usize).copied(),
                ctx.input(|i| i.max_texture_side),
            ));
            if key.scale != wanted || self.page_current(key.page, generation, scale) {
                continue;
            }
            let image = match image {
                Ok(image) => image,
                Err(e) => {
                    self.message = format!("쪽을 표시하지 못했습니다: {e}");
                    continue;
                }
            };
            self.ext.shell.rasters += 1;
            let _ = self.install_page(ctx, key.page, generation, scale, image);
        }
    }

    /// Paper-coloured stand-in for a page whose bitmap is still being made.
    pub(crate) fn blank_page(&mut self, ctx: &egui::Context) -> egui::TextureHandle {
        let colors = self.page_style().1;
        if let Some((at, texture)) = &self.ext.shell.blank
            && *at == colors
        {
            return texture.clone();
        }
        let paper = colors.map_or(Color32::WHITE, |(paper, _)| paper);
        let texture = ctx.load_texture(
            "page-blank",
            egui::ColorImage::new([1, 1], vec![paper]),
            egui::TextureOptions::NEAREST,
        );
        self.ext.shell.blank = Some((colors, texture.clone()));
        texture
    }

    /// With every visible page drawn and a worker free, draw the page after the last
    /// visible one (then the one before the first) so plain scrolling finds it ready.
    /// While the document keeps changing (typing) it waits: each key would otherwise
    /// remake a page nobody sees.
    pub(crate) fn prefetch_pages(&mut self, ctx: &egui::Context) {
        let generation = self.session.generation;
        let seen = self.ext.shell.edit_seen.0;
        if seen != generation {
            let first = seen == u64::MAX;
            self.ext.shell.edit_seen = (generation, (!first).then(std::time::Instant::now));
        }
        if let Some(since) = self.ext.shell.edit_seen.1
            && since.elapsed() < PREFETCH_QUIET
        {
            ctx.request_repaint_after(PREFETCH_QUIET - since.elapsed().min(PREFETCH_QUIET));
            return;
        }
        let pass = ctx.cumulative_pass_nr();
        let (at, now, _) = &self.ext.shell.requested;
        if !self.raster_async()
            || *at != pass
            || self
                .ext
                .shell
                .raster
                .as_ref()
                .is_some_and(|r| !r.busy.is_empty())
        {
            return;
        }
        let (Some(first), Some(last)) = (now.first().copied(), now.last().copied()) else {
            return;
        };
        let scale_of = |app: &Self, page: u32| {
            fit_texture(
                raster_scale(app.zoom, ctx.pixels_per_point()),
                app.page_sizes.1.get(page as usize).copied(),
                ctx.input(|i| i.max_texture_side),
            )
        };
        if now
            .iter()
            .any(|p| !self.page_current(*p, generation, scale_of(self, *p)))
        {
            return;
        }
        let count = self.page_sizes.1.len() as u32;
        let next = [last + 1, first.wrapping_sub(1)]
            .into_iter()
            .filter(|p| *p < count)
            .find(|p| !self.page_current(*p, generation, scale_of(self, *p)));
        if let Some(page) = next {
            let scale = scale_of(self, page);
            self.queue_raster(ctx, page, scale);
        }
    }
}

/// How long the document must stay unchanged before pages out of view are made ahead.
const PREFETCH_QUIET: std::time::Duration = std::time::Duration::from_millis(150);

#[cfg(test)]
mod raster_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn app(pages: usize) -> anyhow::Result<HwpApp> {
        let mut a = HwpApp::with_context(egui::Context::default(), None, None, false)?;
        for i in 0..pages {
            if i > 0 {
                a.session.execute(Command::PageBreak)?;
            }
            a.session
                .execute(Command::Insert(format!("{}쪽 본문", i + 1)))?;
        }
        a.ext.shell.async_raster = true;
        a.page_sizes();
        Ok(a)
    }

    /// One view pass asking for `pages`, then the end-of-frame prefetch.
    fn pass(a: &mut HwpApp, pages: &[u32]) -> Vec<[usize; 2]> {
        let ctx = a.ctx.clone();
        let mut sizes = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
            for page in pages {
                sizes.push(a.shell_page_texture(&ctx, *page).unwrap().size());
            }
            a.prefetch_pages(&ctx);
        });
        output.textures_delta.clear();
        sizes
    }

    fn until(a: &mut HwpApp, pages: &[u32], done: impl Fn(&HwpApp) -> bool) {
        let start = Instant::now();
        while !done(a) {
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "workers never finished"
            );
            std::thread::sleep(Duration::from_millis(5));
            pass(a, pages);
        }
    }

    fn current(a: &HwpApp, page: u32) -> bool {
        let scale = raster_scale(a.zoom, 1.0);
        a.page_current(page, a.session.generation, scale)
    }

    #[test]
    fn a_new_page_shows_blank_paper_then_the_same_bitmap_as_a_direct_render() -> anyhow::Result<()>
    {
        let mut a = app(3)?;
        assert_eq!(
            pass(&mut a, &[0]),
            vec![[1, 1]],
            "blank paper while it renders"
        );
        until(&mut a, &[0], |a| current(a, 0));
        let scale = raster_scale(a.zoom, 1.0);
        let direct = a.shell_page_image(0, scale)?;
        assert_eq!(a.ext.shell.page_images[&0].pixels, direct.pixels);
        assert_eq!(pass(&mut a, &[0]), vec![direct.size]);
        // With page 0 drawn and the workers free, the next page is made ahead.
        until(&mut a, &[0], |a| current(a, 1));
        Ok(())
    }

    #[test]
    fn typing_holds_back_pages_out_of_view_until_it_pauses() -> anyhow::Result<()> {
        let mut a = app(3)?;
        until(&mut a, &[0], |a| current(a, 0) && current(a, 1));
        // An edit: the visible page is remade, the one below waits for a pause.
        a.session.execute(Command::Insert("가".into()))?;
        let edited = Instant::now();
        until(&mut a, &[0], |a| current(a, 0));
        if edited.elapsed() < PREFETCH_QUIET {
            assert!(!current(&a, 1), "made ahead while typing");
        }
        until(&mut a, &[0], |a| current(a, 1));
        assert!(edited.elapsed() >= PREFETCH_QUIET);
        // A page scrolled into view is made at once, pause or not.
        a.session.execute(Command::Insert("나".into()))?;
        until(&mut a, &[0], |a| current(a, 0));
        a.ext.shell.edit_seen.1 = Some(Instant::now() + Duration::from_secs(60));
        pass(&mut a, &[0, 1]);
        let queued = a
            .ext
            .shell
            .raster
            .as_ref()
            .is_some_and(|r| r.busy.iter().any(|k| k.page == 1));
        assert!(
            queued || current(&a, 1),
            "the page in view waits for nothing"
        );
        until(&mut a, &[0, 1], |a| current(a, 1));
        Ok(())
    }

    #[test]
    fn a_zoom_step_keeps_the_old_bitmap_until_the_new_one_arrives() -> anyhow::Result<()> {
        let mut a = app(1)?;
        until(&mut a, &[0], |a| current(a, 0));
        let old = a.pages[&0].0.size();
        a.zoom = 1.4;
        assert_eq!(pass(&mut a, &[0]), vec![old]);
        until(&mut a, &[0], |a| current(a, 0));
        assert!(a.pages[&0].0.size()[0] > old[0]);
        Ok(())
    }

    #[test]
    fn a_bitmap_made_before_an_edit_or_colour_change_is_not_used() -> anyhow::Result<()> {
        let mut a = app(1)?;
        pass(&mut a, &[0]);
        a.session.execute(Command::Insert("바뀐 글".into()))?;
        a.ext.shell.view_color = ViewColor::Dark;
        until(&mut a, &[0], |a| current(a, 0));
        let scale = raster_scale(a.zoom, 1.0);
        let direct = a.shell_page_image(0, scale)?;
        assert_eq!(a.ext.shell.page_images[&0].pixels, direct.pixels);
        Ok(())
    }

    fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
        let lum = |c: [u8; 3]| {
            let [r, g, b] = c.map(|v| {
                let v = f32::from(v) / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            });
            0.2126 * r + 0.7152 * g + 0.0722 * b
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn dark_view_turns_coloured_headings_light_and_keeps_their_hue() {
        let (paper, text) = ViewColor::Dark.colors(Default::default()).unwrap();
        let rgb = |c: Color32| [c.r(), c.g(), c.b()];
        assert_eq!(view_color([255, 255, 255], paper, text), rgb(paper));
        assert_eq!(view_color([0, 0, 0], paper, text), rgb(text));
        // The sample's dark blue title (#234c7d) and body ink (#263445).
        for ink in [[0x23, 0x4c, 0x7d], [0x26, 0x34, 0x45]] {
            let shown = view_color(ink, paper, text);
            assert!(contrast(shown, rgb(paper)) >= 4.5, "{ink:?} -> {shown:?}");
        }
        let [r, g, b] = view_color([0x23, 0x4c, 0x7d], paper, text);
        assert!(b > g && g > r, "still blue: {r} {g} {b}");
    }

    #[test]
    fn view_colours_rewrite_paint_and_add_paper_but_leave_references_alone() {
        let paper = Color32::from_rgb(43, 43, 46);
        let text = Color32::from_rgb(222, 222, 222);
        let svg = r##"<svg width="10" height="10"><use xlink:href="#a1b2c3"/><rect fill="#ffffff" stroke="#000000"/><image href="data:image/png;base64,AAAA"/></svg>"##;
        let out = view_colors_svg(svg, paper, text);
        assert!(out.contains(r##"fill="#2b2b2e"/>"##), "paper rect: {out}");
        assert!(
            out.contains(r##"<rect fill="#2b2b2e" stroke="#dedede"/>"##),
            "{out}"
        );
        assert!(out.contains(r##"xlink:href="#a1b2c3""##), "{out}");
        assert!(out.contains(r##"<image href="data:image/png;base64,AAAA"/>"##));
    }
}
