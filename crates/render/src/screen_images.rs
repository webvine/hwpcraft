//! Owner: shell-page area. Screen-only stand-ins for large embedded raster images.
//!
//! Long documents often draw one large picture on every page: the 381-page handbook
//! puts a 2551×3578 PNG behind each page. resvg decodes and resamples it on every
//! page raster, about 50 ms of a 75 ms raster at 1.0×, on every keystroke.
//!
//! A stand-in is that image drawn once, exactly as the page draws it (same transform
//! and filter), into the device pixels it covers. The result is kept as a small PNG
//! inside an SVG image that uses the original image's own coordinate space, and it is
//! drawn back 1:1 with nearest sampling. The page raster comes out the same; the
//! measured largest channel difference was 0–1.
//!
//! Only [`crate::Renderer::screen`] uses stand-ins. [`crate::Renderer::svg`] (MCP,
//! CLI, previews), printing and PDF output never see them.
//!
//! Stand-ins are chosen while usvg parses, before an image's placement is known. Every
//! parse that used one is therefore checked afterwards. Each substituted image must sit
//! where its stand-in was made for (same scale and sub-pixel offset). If one does not,
//! the page is parsed again with the original images.
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{self, ImageHrefResolver, ImageKind, ImageRendering, Options, Tree};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Images below this many pixels are cheap to draw as they are.
const MIN_SOURCE_PIXELS: f32 = 1_000_000.0;
/// A stand-in larger than this many device pixels is not made (pages are smaller).
const MAX_STANDIN_PIXELS: u64 = 16_000_000;
/// Stand-ins kept, and the PNG bytes they may hold in total (least recently used go).
const MAX_STANDINS: usize = 24;
const MAX_STANDIN_BYTES: usize = 32 << 20;
/// A tag whose stand-in was rejected this often is left alone from then on.
const MAX_MISSES: u32 = 4;
/// Stand-ins waiting for the single worker; past this the oldest request is dropped.
/// Zooming through several steps quickly then makes the current step's first, and a
/// page never holds more than one stand-in raster (up to 64 MB) in the making.
const MAX_QUEUED: usize = 4;

/// Decoded image bytes: content hash and length.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ImageKey(u64, usize);

impl ImageKey {
    fn of(data: &[u8]) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        data.hash(&mut hasher);
        Self(hasher.finish(), data.len())
    }
}

/// Where an image lands on the device grid, up to whole-pixel shifts: the scale of
/// image pixels to device pixels and the sub-pixel offset. A band redraw shifts the
/// page by whole rows, so its placements equal those of the full page.
#[derive(Clone, Copy, Debug)]
struct Placement {
    sx: f32,
    sy: f32,
    fx: f32,
    fy: f32,
    width: f32,
    height: f32,
    mode: ImageRendering,
}

impl Placement {
    /// The placement of an image node at `scale`, if it is axis-aligned.
    fn of(image: &usvg::Image, scale: f32) -> Option<Self> {
        let d = Transform::from_scale(scale, scale).pre_concat(image.abs_transform());
        let size = image.size();
        if d.kx != 0.0 || d.ky != 0.0 || d.sx <= 0.0 || d.sy <= 0.0 || !d.is_finite() {
            return None;
        }
        // An offset just below a whole pixel is that pixel (float error of a crop).
        let frac = |t: f32| {
            let f = t - t.floor();
            if f > 1.0 - Self::PX { f - 1.0 } else { f }
        };
        Some(Self {
            sx: d.sx,
            sy: d.sy,
            fx: frac(d.tx),
            fy: frac(d.ty),
            width: size.width(),
            height: size.height(),
            mode: image.rendering_mode(),
        })
    }
    /// Nearest sampling picks the same source pixel while the error stays far below
    /// half a pixel; edge coverage changes by well under one level.
    const PX: f32 = 0.002;
    fn same(&self, other: &Self) -> bool {
        const PX: f32 = Placement::PX;
        (self.sx - other.sx).abs() * self.width <= PX
            && (self.sy - other.sy).abs() * self.height <= PX
            && (self.fx - other.fx).abs() <= PX
            && (self.fy - other.fy).abs() <= PX
            && self.width == other.width
            && self.height == other.height
            && self.mode == other.mode
    }
    /// Device pixels the image covers: whole pixels, shifted to start at 0.
    fn device_size(&self) -> (u32, u32) {
        let w = (self.fx + self.sx * self.width).ceil();
        let h = (self.fy + self.sy * self.height).ceil();
        (w as u32, h as u32)
    }
    /// Worth a stand-in: a large source drawn into at most half as many pixels.
    fn worth_it(&self) -> bool {
        let (w, h) = self.device_size();
        let device = u64::from(w) * u64::from(h);
        let source = self.width * self.height;
        source >= MIN_SOURCE_PIXELS
            && device > 0
            && device <= MAX_STANDIN_PIXELS
            && (device as f32) * 2.0 <= source
    }
}

struct StandIn {
    key: ImageKey,
    placement: Placement,
    tree: Tree,
    /// The pre-rendered PNG inside `tree`, which identifies it in a parsed page.
    png: Arc<Vec<u8>>,
    last_used: u64,
}

/// What one `<image>` tag of a page resolved to before: keyed by the image bytes,
/// the page scale and the tag's own attributes, which a band redraw does not change.
type TagKey = (ImageKey, u32, u64);

#[derive(Default)]
struct Store {
    standins: Vec<StandIn>,
    /// The placement each tag had when last parsed with the original image.
    tags: HashMap<TagKey, Placement>,
    /// Rejected stand-ins per tag; past `MAX_MISSES` the tag keeps its original.
    misses: HashMap<TagKey, u32>,
    /// Stand-ins waiting to be made, oldest first; the worker takes the newest.
    queue: Vec<Job>,
    /// The stand-in the worker is making now.
    building: Option<(ImageKey, Placement)>,
    /// A worker thread is running (at most one).
    worker: bool,
    tick: u64,
    stats: Stats,
}

/// A stand-in to make: the image, where it lands, and its decoded-on-demand data.
struct Job {
    key: ImageKey,
    placement: Placement,
    kind: ImageKind,
}

impl Store {
    fn bytes(&self) -> usize {
        self.standins.iter().map(|s| s.png.len()).sum()
    }
    fn find(&mut self, key: ImageKey, placement: &Placement) -> Option<&mut StandIn> {
        self.standins
            .iter_mut()
            .find(|s| s.key == key && s.placement.same(placement))
    }
    /// Queue a stand-in unless it exists or is already on its way. True when the
    /// caller should start the worker (none is running).
    fn enqueue(&mut self, job: Job) -> bool {
        let same =
            |key: ImageKey, placement: &Placement| key == job.key && placement.same(&job.placement);
        let known = self.find(job.key, &job.placement).is_some()
            || self.building.as_ref().is_some_and(|(k, p)| same(*k, p))
            || self.queue.iter().any(|j| same(j.key, &j.placement));
        if known {
            return false;
        }
        self.queue.push(job);
        if self.queue.len() > MAX_QUEUED {
            self.queue.remove(0);
            self.stats.dropped += 1;
        }
        !std::mem::replace(&mut self.worker, true)
    }
    /// The worker's next stand-in, newest first; `None` ends the worker.
    fn next_job(&mut self) -> Option<Job> {
        let job = self.queue.pop();
        self.building = job.as_ref().map(|j| (j.key, j.placement));
        if job.is_none() {
            self.worker = false;
        }
        job
    }
    fn insert(&mut self, standin: StandIn) {
        self.standins.push(standin);
        while self.standins.len() > MAX_STANDINS || self.bytes() > MAX_STANDIN_BYTES {
            let Some(oldest) = self
                .standins
                .iter()
                .enumerate()
                .min_by_key(|(_, s)| s.last_used)
                .map(|(i, _)| i)
            else {
                break;
            };
            self.standins.remove(oldest);
            self.stats.evicted += 1;
        }
    }
}

/// Counters for tests and measurements.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Stats {
    /// Stand-ins made.
    pub built: u32,
    /// Images drawn from a stand-in.
    pub used: u32,
    /// Parses done again with the original images after a placement check failed.
    pub reparsed: u32,
    /// Stand-ins dropped to stay within the bounds.
    pub evicted: u32,
    /// Requests dropped from a full queue before they were made.
    pub dropped: u32,
}

/// One screen parse on this thread: what the resolver handed out and saw.
#[derive(Default)]
struct ParseLog {
    scale: u32,
    /// Attribute hashes of the page's data-URI `<image>` tags, in document order.
    hints: Vec<u64>,
    calls: usize,
    /// Original images seen, by data pointer: their key and tag.
    seen: Vec<(usize, TagKey)>,
    /// Stand-ins handed out: their PNG pointer and tag.
    handed: Vec<(usize, TagKey)>,
    originals_only: bool,
}

thread_local! {
    static LOG: RefCell<Option<ParseLog>> = const { RefCell::new(None) };
}

pub(crate) struct ScreenImages {
    store: Arc<Mutex<Store>>,
    options: Options<'static>,
}

impl ScreenImages {
    /// `base` supplies the fonts; the image resolver is the stand-in one.
    pub(crate) fn new(base: &Options<'static>) -> Self {
        let store: Arc<Mutex<Store>> = Arc::default();
        let resolver_store = store.clone();
        let default = ImageHrefResolver::default_data_resolver();
        let resolve_data = Box::new(
            move |mime: &str, data: Arc<Vec<u8>>, options: &Options| -> Option<ImageKind> {
                let kind = default(mime, data.clone(), options);
                let raster = matches!(
                    kind,
                    Some(
                        ImageKind::JPEG(_)
                            | ImageKind::PNG(_)
                            | ImageKind::GIF(_)
                            | ImageKind::WEBP(_)
                    )
                );
                let standin = resolve(&resolver_store, &data, raster);
                standin.or(kind)
            },
        );
        let options = Options {
            fontdb: base.fontdb.clone(),
            font_family: base.font_family.clone(),
            image_href_resolver: ImageHrefResolver {
                resolve_data,
                resolve_string: ImageHrefResolver::default_string_resolver(),
            },
            ..Default::default()
        };
        Self { store, options }
    }

    /// Parse a page for the screen at `scale`, with stand-ins where they are known to
    /// fit, and learn the large images that do not have one yet.
    pub(crate) fn parse(&self, svg: &str, scale: f32) -> anyhow::Result<Tree> {
        let hints = tag_hints(svg);
        let tree = self.parse_once(svg, scale, &hints, false)?;
        if let Some(tree) = tree {
            return Ok(tree);
        }
        if let Ok(mut store) = self.store.lock() {
            store.stats.reparsed += 1;
        }
        self.parse_once(svg, scale, &hints, true)?
            .ok_or_else(|| anyhow::anyhow!("쪽 그림을 다시 읽지 못했습니다."))
    }

    /// `None` when a stand-in turned out not to fit (parse again with originals).
    fn parse_once(
        &self,
        svg: &str,
        scale: f32,
        hints: &[u64],
        originals_only: bool,
    ) -> anyhow::Result<Option<Tree>> {
        LOG.with(|log| {
            *log.borrow_mut() = Some(ParseLog {
                scale: scale.to_bits(),
                hints: hints.to_vec(),
                originals_only,
                ..Default::default()
            })
        });
        let parsed = Tree::from_str(svg, &self.options);
        let log = LOG.with(|log| log.borrow_mut().take()).unwrap_or_default();
        let tree = parsed?;
        Ok(self.check(&tree, scale, &log).then_some(tree))
    }

    /// Verify every stand-in in `tree` and learn its original large images.
    fn check(&self, tree: &Tree, scale: f32, log: &ParseLog) -> bool {
        let mut images = Vec::new();
        collect_images(tree.root(), &mut images);
        let Ok(mut store) = self.store.lock() else {
            return log.handed.is_empty();
        };
        let mut fits = 0;
        let mut ok = true;
        let mut start_worker = false;
        for image in &images {
            let placement = Placement::of(image, scale);
            match image.kind() {
                ImageKind::SVG(inner) => {
                    let Some(ptr) = inner_png(inner) else {
                        continue;
                    };
                    let Some((_, tag)) = log.handed.iter().find(|(p, _)| *p == ptr) else {
                        continue;
                    };
                    let fit = store
                        .standins
                        .iter()
                        .find(|s| Arc::as_ptr(&s.png) as usize == ptr)
                        .zip(placement)
                        .is_some_and(|(s, p)| s.placement.same(&p));
                    if fit {
                        fits += 1;
                    } else {
                        ok = false;
                        *store.misses.entry(*tag).or_default() += 1;
                        store.tags.remove(tag);
                    }
                }
                ImageKind::JPEG(data)
                | ImageKind::PNG(data)
                | ImageKind::GIF(data)
                | ImageKind::WEBP(data) => {
                    let ptr = Arc::as_ptr(data) as usize;
                    let Some((_, tag)) = log.seen.iter().find(|(p, _)| *p == ptr) else {
                        continue;
                    };
                    if tag.2 == u64::MAX {
                        // No tag to find it by on the next parse.
                        continue;
                    }
                    let Some(placement) = placement.filter(Placement::worth_it) else {
                        continue;
                    };
                    store.tags.insert(*tag, placement);
                    start_worker |= store.enqueue(Job {
                        key: tag.0,
                        placement,
                        kind: image.kind().clone(),
                    });
                }
            }
        }
        drop(store);
        if start_worker {
            spawn_worker(self.store.clone());
        }
        // A stand-in that did not show up in the tree (in a pattern or mask, which
        // this check does not walk) cannot be verified either.
        ok && fits == log.handed.len()
    }

    pub(crate) fn stats(&self) -> Stats {
        self.store.lock().map(|s| s.stats).unwrap_or_default()
    }

    /// Wait until no stand-in is being made (tests and measurements).
    pub(crate) fn settle(&self, timeout: std::time::Duration) -> bool {
        let until = std::time::Instant::now() + timeout;
        loop {
            let idle = self
                .store
                .lock()
                .map(|s| s.queue.is_empty() && !s.worker)
                .unwrap_or(true);
            if idle {
                return true;
            }
            if std::time::Instant::now() >= until {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

/// The resolver's part: hand out a stand-in for this tag if one fits, else remember
/// the original so the check can learn where it lands.
fn resolve(store: &Mutex<Store>, data: &Arc<Vec<u8>>, raster: bool) -> Option<ImageKind> {
    let (scale, hint, originals_only) = LOG.with(|log| {
        let mut log = log.borrow_mut();
        let log = log.as_mut()?;
        let hint = log.hints.get(log.calls).copied();
        log.calls += 1;
        Some((log.scale, hint, log.originals_only))
    })?;
    if !raster {
        return None;
    }
    let key = ImageKey::of(data);
    let tag = (key, scale, hint.unwrap_or(u64::MAX));
    let ptr = Arc::as_ptr(data) as usize;
    let standin = (!originals_only && hint.is_some())
        .then(|| {
            let mut store = store.lock().ok()?;
            if store.misses.get(&tag).copied().unwrap_or(0) >= MAX_MISSES {
                return None;
            }
            let placement = *store.tags.get(&tag)?;
            store.tick += 1;
            let tick = store.tick;
            let standin = store.find(key, &placement)?;
            standin.last_used = tick;
            let handed = (standin.tree.clone(), Arc::as_ptr(&standin.png) as usize);
            store.stats.used += 1;
            Some(handed)
        })
        .flatten();
    LOG.with(|log| {
        if let Some(log) = log.borrow_mut().as_mut() {
            match &standin {
                Some((_, png)) => log.handed.push((*png, tag)),
                None => log.seen.push((ptr, tag)),
            }
        }
    });
    standin.map(|(tree, _)| ImageKind::SVG(tree))
}

/// Attribute text of each data-URI `<image>` tag before its `href`, with how many
/// identical tags came before it on the page, hashed, in document order.
fn tag_hints(svg: &str) -> Vec<u64> {
    use std::hash::{Hash, Hasher};
    let mut hints = Vec::new();
    let mut repeats: HashMap<u64, u32> = HashMap::new();
    let mut rest = svg;
    while let Some(i) = rest.find("<image") {
        rest = &rest[i + 6..];
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        if let Some(href) = tag.find("href=\"data:").or_else(|| tag.find("href='data:")) {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            tag[..href].hash(&mut hasher);
            let text = hasher.finish();
            let seen = repeats.entry(text).or_default();
            (text, *seen).hash(&mut hasher);
            *seen += 1;
            hints.push(hasher.finish());
        }
        rest = &rest[end..];
    }
    hints
}

fn collect_images<'a>(group: &'a usvg::Group, out: &mut Vec<&'a usvg::Image>) {
    for node in group.children() {
        match node {
            usvg::Node::Image(image) => out.push(image),
            usvg::Node::Group(group) => collect_images(group, out),
            usvg::Node::Path(_) | usvg::Node::Text(_) => {}
        }
    }
}

/// The PNG data pointer inside a stand-in tree.
fn inner_png(tree: &Tree) -> Option<usize> {
    let mut images = Vec::new();
    collect_images(tree.root(), &mut images);
    match images.first()?.kind() {
        ImageKind::PNG(data) => Some(Arc::as_ptr(data) as usize),
        _ => None,
    }
}

/// The single worker: makes queued stand-ins, newest first, until the queue is empty.
fn spawn_worker(store: Arc<Mutex<Store>>) {
    let worker = store.clone();
    let spawned = std::thread::Builder::new()
        .name("hwpcraft-screen-image".into())
        .spawn(move || {
            loop {
                let Some(job) = worker.lock().ok().and_then(|mut s| s.next_job()) else {
                    return;
                };
                let built = build(&job.placement, job.kind);
                let Ok(mut store) = worker.lock() else {
                    return;
                };
                store.building = None;
                if let Some((tree, png)) = built {
                    store.stats.built += 1;
                    let tick = store.tick;
                    store.insert(StandIn {
                        key: job.key,
                        placement: job.placement,
                        tree,
                        png,
                        last_used: tick,
                    });
                }
            }
        });
    if spawned.is_err()
        && let Ok(mut store) = store.lock()
    {
        // No thread: the pages keep their original images; the next parse retries.
        store.stats.dropped += store.queue.len() as u32;
        store.queue.clear();
        store.worker = false;
    }
}

/// An image that resolves every data URI to `kind`, for the two helper trees.
fn single_image(kind: ImageKind) -> Options<'static> {
    Options {
        image_href_resolver: ImageHrefResolver {
            resolve_data: Box::new(move |_, _, _| Some(kind.clone())),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    }
}

fn mode_name(mode: ImageRendering) -> &'static str {
    match mode {
        ImageRendering::OptimizeQuality => "optimizeQuality",
        ImageRendering::OptimizeSpeed => "optimizeSpeed",
        ImageRendering::Smooth => "smooth",
        ImageRendering::HighQuality => "high-quality",
        ImageRendering::CrispEdges => "crisp-edges",
        ImageRendering::Pixelated => "pixelated",
    }
}

/// Draw `kind` as a page at `placement` would, then wrap the pixels as a stand-in.
fn build(placement: &Placement, kind: ImageKind) -> Option<(Tree, Arc<Vec<u8>>)> {
    let (w, h) = placement.device_size();
    let p = placement;
    // 1. The image alone, with the page's scale and sub-pixel offset.
    let draw = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{w}' height='{h}'>\
         <g transform='matrix({} 0 0 {} {} {})'>\
         <image width='{}' height='{}' preserveAspectRatio='none' image-rendering='{}' href='data:image/png;base64,AA=='/>\
         </g></svg>",
        p.sx,
        p.sy,
        p.fx,
        p.fy,
        p.width,
        p.height,
        mode_name(p.mode)
    );
    let tree = Tree::from_str(&draw, &single_image(kind)).ok()?;
    let mut pixmap = Pixmap::new(w, h)?;
    resvg::render(&tree, Transform::identity(), &mut pixmap.as_mut());
    let png = Arc::new(pixmap.encode_png().ok()?);
    // 2. Those pixels in the original image's space: device pixels 0..w map back to
    //    image units (0 - fx) / sx .. (w - fx) / sx; nearest sampling draws them 1:1.
    let wrap = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{iw}' height='{ih}' viewBox='0 0 {iw} {ih}'>\
         <image x='{}' y='{}' width='{}' height='{}' preserveAspectRatio='none' image-rendering='optimizeSpeed' href='data:image/png;base64,AA=='/>\
         </svg>",
        -p.fx / p.sx,
        -p.fy / p.sy,
        w as f32 / p.sx,
        h as f32 / p.sy,
        iw = p.width,
        ih = p.height
    );
    let tree = Tree::from_str(&wrap, &single_image(ImageKind::PNG(png.clone()))).ok()?;
    Some((tree, png))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::Renderer;
    use std::time::Duration;

    fn base64(bytes: &[u8]) -> String {
        const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity(bytes.len() * 4 / 3 + 4);
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    /// A large picture with fine stripes and soft colour, so resampling matters.
    fn picture() -> String {
        let (w, h) = (1600u32, 2200u32);
        let mut pixmap = Pixmap::new(w, h).unwrap();
        for (i, px) in pixmap.data_mut().chunks_mut(4).enumerate() {
            let (x, y) = (i as u32 % w, i as u32 / w);
            let stripe = if (x + y) / 3 % 2 == 0 { 40 } else { 220 };
            px.copy_from_slice(&[stripe, (x * 255 / w) as u8, (y * 255 / h) as u8, 255]);
        }
        format!(
            "data:image/png;base64,{}",
            base64(&pixmap.encode_png().unwrap())
        )
    }

    fn page(view: &str, body: &str) -> String {
        format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='400' height='{}' viewBox='{view}'>\
             <rect x='0' y='0' width='400' height='550' fill='#ffffff'/>{body}</svg>",
            view.split(' ').nth(3).unwrap()
        )
    }

    fn largest_difference(a: &crate::Page, b: &crate::Page) -> u8 {
        assert_eq!((a.width, a.height), (b.width, b.height));
        a.rgba
            .iter()
            .zip(&b.rgba)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn stand_ins_draw_the_same_pixels_as_the_original_image() -> anyhow::Result<()> {
        let data = picture();
        let image = format!(
            "<image x='3.3' y='5.7' width='380.4' height='520.2' preserveAspectRatio='none' href='{data}'/>"
        );
        let full = page("0 0 400 550", &image);
        let r = Renderer::default();
        let original = r.svg(&full, 1.0)?;
        // The first screen raster draws the original and has the stand-in made.
        let first = r.screen(&full, 1.0)?;
        assert_eq!(largest_difference(&original, &first), 0);
        assert!(r.settle_screen_images(Duration::from_secs(60)));
        assert_eq!(r.screen_image_stats().built, 1);
        // Later rasters use it, with the same pixels (a comment defeats the page cache).
        let again = r.screen(&full.replace("<rect", "<!-- 2 --><rect"), 1.0)?;
        assert_eq!(r.screen_image_stats().used, 1);
        assert!(largest_difference(&original, &again) <= 1);
        // A band of whole rows (as a caret band redraw crops the page) uses it too.
        let band = page("0 120 400 200", &image);
        let cropped = r.screen(&band, 1.0)?;
        assert_eq!(r.screen_image_stats().used, 2);
        assert_eq!(r.screen_image_stats().reparsed, 0);
        assert!(largest_difference(&r.svg(&band, 1.0)?, &cropped) <= 1);
        // `svg` output (MCP, CLI, previews) is untouched by the stand-ins.
        let fresh = Renderer::default().svg(&full, 1.0)?;
        assert_eq!(
            r.svg(&full.replace("<rect", "<!-- 3 --><rect"), 1.0)?.rgba,
            fresh.rgba
        );
        Ok(())
    }

    #[test]
    fn a_stand_in_is_only_used_where_it_was_made_for() -> anyhow::Result<()> {
        let data = picture();
        let tag =
            format!("<image width='1600' height='2200' preserveAspectRatio='none' href='{data}'/>");
        // The same picture twice: scaled into the page, and cropped by a nested view.
        let two = page(
            "0 0 400 550",
            &format!(
                "<g transform='matrix(0.2 0 0 0.2 10.25 4.5)'>{tag}</g>\
                 <svg x='200' y='300' width='150' height='100' viewBox='400 600 1200 800'>{tag}</svg>"
            ),
        );
        let r = Renderer::default();
        r.screen(&two, 1.0)?;
        assert!(r.settle_screen_images(Duration::from_secs(60)));
        let again = r.screen(&two.replace("<rect", "<!-- 2 --><rect"), 1.0)?;
        assert!(largest_difference(&r.svg(&two, 1.0)?, &again) <= 1);
        assert_eq!(r.screen_image_stats().reparsed, 0);
        // The same tag under another transform: the check rejects the stand-in and the
        // page is drawn from the original picture.
        let moved = page(
            "0 0 400 550",
            &format!("<g transform='matrix(0.21 0 0 0.21 10.5 4.5)'>{tag}</g>"),
        );
        let drawn = r.screen(&moved, 1.0)?;
        assert_eq!(r.screen_image_stats().reparsed, 1);
        assert!(largest_difference(&r.svg(&moved, 1.0)?, &drawn) <= 1);
        Ok(())
    }

    #[test]
    fn one_worker_makes_stand_ins_newest_first_from_a_bounded_queue() {
        let job = |i: u64| Job {
            key: ImageKey(i, 1),
            placement: Placement {
                sx: 0.25,
                sy: 0.25,
                fx: 0.0,
                fy: 0.0,
                width: 4000.0,
                height: 4000.0,
                mode: ImageRendering::OptimizeQuality,
            },
            kind: ImageKind::PNG(Arc::new(Vec::new())),
        };
        let mut store = Store::default();
        assert!(store.enqueue(job(0)), "the first request starts the worker");
        for i in 1..10 {
            assert!(!store.enqueue(job(i)), "never a second worker");
        }
        store.enqueue(job(9));
        assert_eq!(
            store.queue.len(),
            MAX_QUEUED,
            "bounded; a queued request is not doubled"
        );
        assert_eq!(store.stats.dropped as usize, 10 - MAX_QUEUED);
        // The newest request (the current zoom step) is made first.
        let first = store.next_job().map(|j| j.key);
        assert_eq!(first, Some(ImageKey(9, 1)));
        assert!(
            !store.enqueue(job(9)),
            "the one being made is not queued again"
        );
        while store.next_job().is_some() {}
        assert!(!store.worker, "the worker ends with the queue");
        assert!(store.enqueue(job(20)), "a later request starts it again");
    }

    #[test]
    fn stand_ins_stay_within_their_bounds() -> anyhow::Result<()> {
        let tree = Tree::from_str(
            "<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'/>",
            &Options::default(),
        )?;
        let mut store = Store::default();
        for i in 0..(MAX_STANDINS + 5) {
            store.insert(StandIn {
                key: ImageKey(i as u64, 1),
                placement: Placement {
                    sx: 1.0,
                    sy: 1.0,
                    fx: 0.0,
                    fy: 0.0,
                    width: 1.0,
                    height: 1.0,
                    mode: ImageRendering::OptimizeQuality,
                },
                tree: tree.clone(),
                png: Arc::new(vec![0; 2 << 20]),
                last_used: i as u64,
            });
        }
        assert!(store.standins.len() <= MAX_STANDINS);
        assert!(store.bytes() <= MAX_STANDIN_BYTES);
        assert_eq!(
            store.stats.evicted as usize,
            MAX_STANDINS + 5 - store.standins.len()
        );
        // The least recently used went first.
        assert!(store.standins.iter().all(|s| s.last_used >= 5));
        Ok(())
    }
}
