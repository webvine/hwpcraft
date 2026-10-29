#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use anyhow::{Result, anyhow};
use hwpcraft_doc::DocumentCore;
mod screen_images;
pub use screen_images::Stats as ScreenImageStats;
pub struct Renderer {
    options: resvg::usvg::Options<'static>,
    /// Screen-only stand-ins for large embedded images (see [`Renderer::screen`]).
    screen: screen_images::ScreenImages,
    /// Recently rasterized pages keyed by (SVG content hash, scale). After an edit
    /// only the edited page's SVG changes, so the other visible pages come back
    /// from here instead of being parsed and rasterized again.
    recent: std::sync::Mutex<std::collections::VecDeque<Cached>>,
}

/// How many rasterized pages a renderer keeps (a page at 1.25x is about 6 MB).
const RECENT_PAGES: usize = 6;

struct Cached {
    /// SVG hash and length, scale, and whether it is a screen raster.
    key: (u64, usize, u32, bool),
    width: u32,
    height: u32,
    rgba: std::sync::Arc<Vec<u8>>,
}
pub struct Page {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub svg: String,
}
impl Default for Renderer {
    fn default() -> Self {
        let options = options(true);
        Self {
            screen: screen_images::ScreenImages::new(&options),
            options,
            recent: Default::default(),
        }
    }
}

/// Parse options with the system fonts and the Korean default family; `share` uses
/// the process-wide database whose font files are mapped once (see [`share_font_files`]).
fn options(share: bool) -> resvg::usvg::Options<'static> {
    let (fontdb, family) = if share {
        system_fonts().clone()
    } else {
        font_database(false)
    };
    resvg::usvg::Options {
        fontdb,
        font_family: family,
        ..Default::default()
    }
}

/// The shared font database, built on first use and reused by every renderer in the
/// process (one per MCP document worker, previews, the canvas): scanning and mapping
/// the system fonts again for each renderer cost about 60 ms and several MB.
fn system_fonts() -> &'static (std::sync::Arc<resvg::usvg::fontdb::Database>, String) {
    static FONTS: std::sync::OnceLock<(std::sync::Arc<resvg::usvg::fontdb::Database>, String)> =
        std::sync::OnceLock::new();
    FONTS.get_or_init(|| font_database(true))
}

/// System fonts with the Korean family as the generic serif/sans-serif fallback.
fn font_database(share: bool) -> (std::sync::Arc<resvg::usvg::fontdb::Database>, String) {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    if share {
        share_font_files(&mut db);
    }
    let family = installed_korean_font(&db);
    db.set_sans_serif_family(family.clone());
    db.set_serif_family(family.clone());
    (std::sync::Arc::new(db), family)
}

/// Map every system font file once for the renderer's lifetime.
///
/// usvg asks fontdb for face data per glyph (shaping, outlines, fallback checks). For
/// a file-backed face fontdb opens and maps the file again on every such call, which
/// was about 0.23 s of each page parse on a 381-page document. A shared mapping keeps
/// the bytes available instead. Face ids and order do not change, so font selection is
/// exactly as before. A face whose file cannot be mapped stays file-backed.
fn share_font_files(db: &mut resvg::usvg::fontdb::Database) {
    let ids: Vec<_> = db
        .faces()
        .filter(|face| matches!(face.source, resvg::usvg::fontdb::Source::File(_)))
        .map(|face| face.id)
        .collect();
    for id in ids {
        // SAFETY: the file is mapped read-only and the mapping lives as long as the
        // database. `make_shared_face_data` is unsafe because another process may
        // change the file while it is mapped and the change shows through the mapping.
        // Risk if a font file changes during a session (a font update or uninstall):
        // - rewritten in place: glyph reads see other bytes; ttf-parser bounds-checks
        //   every read, so text may draw with wrong glyphs until restart, no crash;
        // - truncated: on Unix, touching pages past the new end raises SIGBUS and ends
        //   the process; Windows refuses to truncate a file with a mapped view.
        // System font folders are not rewritten while in use in normal operation.
        // Faces of one collection file share one mapping; shared faces return early.
        #[allow(unsafe_code)]
        unsafe {
            db.make_shared_face_data(id);
        }
    }
}

/// First installed family from the OS candidate list, else the first candidate.
/// Latin families tried when no Korean candidate is installed, before any face.
const TEXT_FALLBACKS: &[&str] = &[
    "Noto Sans",
    "DejaVu Sans",
    "Liberation Sans",
    "Ubuntu",
    "Cantarell",
    "Arial",
    "Helvetica",
];

/// The first installed OS Korean candidate. Without one (common on minimal Linux
/// installs), an installed text font: naming a missing family as the default made
/// every unresolved document font, Latin and digits included, render as nothing.
/// usvg's per-character fallback still finds any other face that covers Hangul.
fn installed_korean_font(db: &resvg::usvg::fontdb::Database) -> String {
    let installed = |name: &str| {
        db.faces()
            .any(|face| face.families.iter().any(|(n, _)| n == name))
    };
    let candidates = hwpcraft_doc::fonts::KOREAN_CANDIDATES;
    candidates
        .iter()
        .chain(TEXT_FALLBACKS)
        .find(|name| installed(name))
        .map(|name| (*name).to_owned())
        .or_else(|| {
            db.faces()
                .find_map(|face| face.families.first().map(|(n, _)| n.clone()))
        })
        .unwrap_or_else(|| candidates[0].to_owned())
}

/// The font family HwpCraft uses for text on this computer (probed once): an OS
/// Korean font when one is installed, otherwise an installed text font.
pub fn default_korean_font() -> &'static str {
    static FONT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        installed_korean_font(&db)
    })
}

/// Whether one of the OS Korean candidate fonts is installed. Without one, Hangul
/// renders only if some other installed font covers it.
pub fn korean_font_installed() -> bool {
    hwpcraft_doc::fonts::KOREAN_CANDIDATES.contains(&default_korean_font())
}

/// Symbol and emoji fonts installed on this computer, as (file, index) pairs, for
/// UI toolkits that need wider glyph coverage than the text font (문자표).
pub fn symbol_font_faces() -> Vec<(std::path::PathBuf, u32)> {
    const FAMILIES: &[&str] = &[
        "Segoe UI Symbol",
        "Segoe UI Emoji",
        "Cambria Math",
        "Apple Symbols",
        "Noto Sans Symbols",
        "Noto Sans Symbols 2",
        "Noto Sans Math",
        "Noto Sans CJK KR",
        "Symbola",
        "DejaVu Sans",
    ];
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    let mut out = Vec::new();
    for family in FAMILIES {
        let face = db
            .faces()
            .filter(|f| f.families.iter().any(|(n, _)| n == family))
            .find(|f| f.style == resvg::usvg::fontdb::Style::Normal && f.weight.0 <= 500);
        if let Some(face) = face
            && let resvg::usvg::fontdb::Source::File(path) = &face.source
            && !out
                .iter()
                .any(|(p, _): &(std::path::PathBuf, u32)| p == path)
        {
            out.push((path.clone(), face.index));
        }
    }
    out
}

/// File and collection index of the regular face of [`default_korean_font`], for
/// UI toolkits that load font bytes themselves (egui). `None` without a file source.
pub fn korean_font_face() -> Option<(std::path::PathBuf, u32)> {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    let family = installed_korean_font(&db);
    let mut faces: Vec<_> = db
        .faces()
        .filter(|face| face.families.iter().any(|(n, _)| *n == family))
        .collect();
    // Prefer the upright regular face.
    faces.sort_by_key(|f| {
        (
            f.style != resvg::usvg::fontdb::Style::Normal,
            (f.weight.0 as i32 - 400).abs(),
        )
    });
    faces.into_iter().find_map(|face| match &face.source {
        resvg::usvg::fontdb::Source::File(path) => Some((path.clone(), face.index)),
        _ => None,
    })
}
impl Renderer {
    /// Installed font families for font pickers: every localized name, sorted with
    /// Korean (Hangul) names first, then the rest alphabetically, without duplicates.
    pub fn font_families(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .options
            .fontdb
            .faces()
            .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
            .filter(|name| !name.is_empty() && !name.starts_with('@'))
            .collect();
        let korean = |s: &str| s.chars().any(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c));
        names.sort_by(|a, b| korean(b).cmp(&korean(a)).then_with(|| a.cmp(b)));
        names.dedup();
        names
    }
    pub fn page(&self, core: &DocumentCore, index: u32, scale: f32) -> Result<Page> {
        let svg = core
            .render_page_svg_native(index)
            .map_err(|e| anyhow!("{e}"))?;
        self.svg(&svg, scale)
    }
    pub fn svg(&self, svg: &str, scale: f32) -> Result<Page> {
        self.cached(svg, scale, false)
    }
    /// Like [`Renderer::svg`] for the document canvas: large embedded images are drawn
    /// from pre-rendered stand-ins that give the same pixels (measured largest channel
    /// difference 0–1) without decoding the image again for every raster. MCP, CLI,
    /// printing and PDF output use `svg` and never see a stand-in.
    pub fn screen(&self, svg: &str, scale: f32) -> Result<Page> {
        self.cached(svg, scale, true)
    }
    /// Stand-in counters (made, used, reparsed, evicted), for tests and measurements.
    pub fn screen_image_stats(&self) -> ScreenImageStats {
        self.screen.stats()
    }
    /// Wait up to `timeout` until no stand-in is being made in the background; true
    /// when none is. Stand-ins are made on a worker thread so a first screen is not
    /// held up; tests and measurements wait here to see them in use.
    pub fn settle_screen_images(&self, timeout: std::time::Duration) -> bool {
        self.screen.settle(timeout)
    }
    fn cached(&self, svg: &str, scale: f32, screen: bool) -> Result<Page> {
        let scale = scale.clamp(0.25, 2.0);
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            svg.hash(&mut hasher);
            (hasher.finish(), svg.len(), scale.to_bits(), screen)
        };
        if let Ok(recent) = self.recent.lock()
            && let Some(hit) = recent.iter().find(|c| c.key == key)
        {
            return Ok(Page {
                width: hit.width,
                height: hit.height,
                rgba: hit.rgba.as_ref().clone(),
                svg: svg.to_owned(),
            });
        }
        let page = self.rasterize(svg, scale, screen)?;
        if let Ok(mut recent) = self.recent.lock() {
            if recent.len() >= RECENT_PAGES {
                recent.pop_front();
            }
            recent.push_back(Cached {
                key,
                width: page.width,
                height: page.height,
                rgba: std::sync::Arc::new(page.rgba.clone()),
            });
        }
        Ok(page)
    }
    fn rasterize(&self, svg: &str, scale: f32, screen: bool) -> Result<Page> {
        let thin = hairlines(svg, 1.0 / scale);
        let tree = if screen {
            self.screen.parse(&thin, scale)?
        } else {
            resvg::usvg::Tree::from_str(&thin, &self.options)?
        };
        let size = tree
            .size()
            .to_int_size()
            .scale_by(scale)
            .ok_or_else(|| anyhow!("쪽 크기를 읽을 수 없습니다."))?;
        if u64::from(size.width()) * u64::from(size.height()) > 24_000_000 {
            return Err(anyhow!("쪽 이미지가 너무 큽니다."));
        }
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
            .ok_or_else(|| anyhow!("쪽 이미지 메모리가 부족합니다."))?;
        pixmap.fill(resvg::tiny_skia::Color::WHITE);
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        Ok(Page {
            width: size.width(),
            height: size.height(),
            rgba: pixmap.data().to_vec(),
            svg: svg.to_owned(),
        })
    }
}

/// Rasterize a text-free SVG (the app icon) to a `size`×`size` straight-alpha RGBA
/// image, keeping transparency. Used for the window icon on Windows and Linux and
/// for the macOS / Linux package icons; needs no system fonts.
pub fn rasterize_icon(svg: &str, size: u32) -> Result<Vec<u8>> {
    if !(16..=1024).contains(&size) {
        return Err(anyhow!("아이콘 크기는 16–1024 픽셀입니다: {size}"));
    }
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())?;
    let extent = tree.size().width().max(tree.size().height());
    if !extent.is_finite() || extent <= 0.0 {
        return Err(anyhow!("아이콘 크기를 읽을 수 없습니다."));
    }
    let scale = size as f32 / extent;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)
        .ok_or_else(|| anyhow!("아이콘 이미지 메모리가 부족합니다."))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia stores premultiplied colour; icon consumers expect straight alpha.
    Ok(pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect())
}

/// Like Hangul's screen view, draw lines thinner than one device pixel as a crisp
/// one-pixel line instead of a faint anti-aliased one. Only the on-screen raster is
/// affected; saved documents and PDF export keep the true widths.
fn hairlines(svg: &str, min: f32) -> String {
    const KEY: &str = "stroke-width=\"";
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(i) = rest.find(KEY) {
        let value_start = i + KEY.len();
        out.push_str(&rest[..value_start]);
        rest = &rest[value_start..];
        let Some(end) = rest.find('"') else {
            break;
        };
        match rest[..end].trim().parse::<f32>() {
            Ok(w) if w > 0.0 && w < min => out.push_str(&format!("{min:.3}")),
            _ => out.push_str(&rest[..end]),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use resvg::usvg::fontdb::Source;

    /// The renderer as it was before font files were shared.
    fn unshared() -> Renderer {
        let options = options(false);
        Renderer {
            screen: screen_images::ScreenImages::new(&options),
            options,
            recent: Default::default(),
        }
    }

    #[test]
    fn identical_pages_come_back_from_the_recent_cache() -> anyhow::Result<()> {
        let renderer = Renderer::default();
        let core = sample_core()?;
        let svg = core
            .render_page_svg_native(0)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let first = renderer.svg(&svg, 1.25)?;
        let cached = renderer.svg(&svg, 1.25)?;
        assert_eq!((first.width, first.height), (cached.width, cached.height));
        assert!(first.rgba == cached.rgba);
        // The cache must not answer for another scale or other content.
        let fresh = Renderer::default();
        for (text, scale) in [(svg.as_str(), 1.0), (svg.as_str(), 1.5)] {
            let a = renderer.svg(text, scale)?;
            let b = fresh.svg(text, scale)?;
            assert_eq!((a.width, a.height), (b.width, b.height));
            assert!(a.rgba == b.rgba);
        }
        let changed = svg.replacen(
            "</svg>",
            "<rect x=\"10\" y=\"10\" width=\"40\" height=\"40\" fill=\"#d00\"/></svg>",
            1,
        );
        assert!(renderer.svg(&changed, 1.25)?.rgba != first.rgba);
        let recent = renderer
            .recent
            .lock()
            .map_err(|_| anyhow::anyhow!("poisoned"))?;
        assert!(recent.len() <= RECENT_PAGES);
        Ok(())
    }

    #[test]
    fn the_recent_cache_is_bounded() -> anyhow::Result<()> {
        let renderer = Renderer::default();
        for i in 0..RECENT_PAGES + 3 {
            let svg = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"20\"><rect width=\"{}\" height=\"5\" fill=\"#000\"/></svg>",
                i + 1
            );
            renderer.svg(&svg, 1.0)?;
        }
        let recent = renderer
            .recent
            .lock()
            .map_err(|_| anyhow::anyhow!("poisoned"))?;
        assert_eq!(recent.len(), RECENT_PAGES);
        Ok(())
    }

    /// `HWPCRAFT_PERF_SAMPLE=<hwp> cargo test --release -p hwpcraft-render -- --ignored perf_recent`
    #[test]
    #[ignore = "measurement on a local sample document"]
    fn perf_recent_page_cache() -> anyhow::Result<()> {
        let Some(path) = std::env::var_os("HWPCRAFT_PERF_SAMPLE") else {
            return Ok(());
        };
        let core = hwpcraft_formats::open(std::path::Path::new(&path))?;
        let renderer = Renderer::default();
        for page in [0_u32, 15, 64] {
            let svg = core
                .render_page_svg_native(page)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let start = std::time::Instant::now();
            renderer.svg(&svg, 1.25)?;
            let cold = start.elapsed();
            let start = std::time::Instant::now();
            renderer.svg(&svg, 1.25)?;
            let warm = start.elapsed();
            eprintln!(
                "page {page}: svg {} KB, first {cold:?}, repeat {warm:?}",
                svg.len() / 1024
            );
        }
        Ok(())
    }

    fn sample_core() -> anyhow::Result<DocumentCore> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../samples/HwpCraft-시작하기.hwpx");
        hwpcraft_formats::open(&path)
    }

    #[test]
    fn renderers_share_one_font_database() {
        let (a, b) = (Renderer::default(), Renderer::default());
        assert!(std::sync::Arc::ptr_eq(&a.options.fontdb, &b.options.fontdb));
        assert_eq!(a.options.font_family, b.options.font_family);
        assert_eq!(a.font_families(), unshared().font_families());
        // The generic families point at the Korean default, as before sharing.
        let db = &a.options.fontdb;
        assert_eq!(
            db.family_name(&resvg::usvg::fontdb::Family::SansSerif),
            a.options.font_family
        );
        assert_eq!(
            db.family_name(&resvg::usvg::fontdb::Family::Serif),
            a.options.font_family
        );
    }

    #[test]
    fn shared_font_files_keep_face_ids_and_order() {
        let plain = options(false);
        let shared = options(true);
        let faces = |o: &resvg::usvg::Options| -> Vec<_> {
            o.fontdb
                .faces()
                .map(|f| (f.id, f.index, f.families.clone(), f.weight, f.style))
                .collect()
        };
        assert_eq!(faces(&plain), faces(&shared));
        // Every file-backed face is now shared (a file that cannot be mapped would
        // stay file-backed; system fonts are readable).
        let file_backed = shared
            .fontdb
            .faces()
            .filter(|f| matches!(f.source, Source::File(_)))
            .count();
        let mapped = shared
            .fontdb
            .faces()
            .filter(|f| matches!(f.source, Source::SharedFile(..)))
            .count();
        assert_eq!(file_backed, 0, "{mapped} mapped");
        assert_eq!(mapped, plain.fontdb.len());
    }

    #[test]
    fn shared_font_files_render_identical_pixels() -> anyhow::Result<()> {
        let (before, after) = (unshared(), Renderer::default());
        let core = sample_core()?;
        let mut svgs = vec![core.render_page_svg_native(0).map_err(|e| anyhow!("{e}"))?];
        // Mixed scripts force fallback fonts: Latin, hanja, symbols, emoji, kana.
        svgs.push(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="420" height="120">
<text x="8" y="30" font-family="함초롬바탕" font-size="18">한글 Latin 漢字 ★☎♪ ①㉠ ÅΩ</text>
<text x="8" y="70" font-family="NoSuchFont" font-size="18" font-weight="bold">굵게 ひらがな 😀 ∑√∞</text>
<text x="8" y="105" font-family="sans-serif" font-size="14" font-style="italic">기울임 italic 1234</text>
</svg>"#
                .to_owned(),
        );
        for (i, svg) in svgs.iter().enumerate() {
            for scale in [1.0, 1.25] {
                let a = before.svg(svg, scale)?;
                // Twice: the first call and a later one reuse the same mappings.
                for _ in 0..2 {
                    let b = after.svg(svg, scale)?;
                    assert_eq!((a.width, a.height), (b.width, b.height));
                    assert!(a.rgba == b.rgba, "svg {i} at {scale}: pixels differ");
                }
            }
        }
        Ok(())
    }

    /// `HWPCRAFT_PERF_SAMPLE=<file> cargo test --release -p hwpcraft-render --lib
    /// perf_ -- --ignored --nocapture`: page rasterisation before and after sharing.
    #[test]
    #[ignore]
    fn perf_shared_font_files() -> anyhow::Result<()> {
        let Some(path) = std::env::var_os("HWPCRAFT_PERF_SAMPLE") else {
            return Ok(());
        };
        let mut core = hwpcraft_formats::open(std::path::Path::new(&path))?;
        let pages = core.page_count().min(40);
        let svgs: Vec<String> = (0..pages)
            .map(|p| core.render_page_svg_native(p).map_err(|e| anyhow!("{e}")))
            .collect::<Result<_>>()?;
        let t = std::time::Instant::now();
        let after = Renderer::default();
        println!("Renderer::default with shared files: {:?}", t.elapsed());
        let before = unshared();
        for (label, r) in [("before", &before), ("after", &after)] {
            let t = std::time::Instant::now();
            for svg in &svgs {
                r.svg(svg, 1.25)?;
            }
            println!(
                "{label}: {pages} pages parse+raster at 1.25: {:?}",
                t.elapsed()
            );
        }
        // Desktop edit: type a character, then redraw a visible text-heavy page.
        let heavy = (0..svgs.len())
            .max_by_key(|&i| svgs[i].matches("<text").count())
            .unwrap_or(0) as u32;
        for (label, r) in [("before", &before), ("after", &after)] {
            let mut total = std::time::Duration::ZERO;
            for _ in 0..3 {
                core.insert_text_native(0, 0, 0, "x")
                    .map_err(|e| anyhow!("{e}"))?;
                let t = std::time::Instant::now();
                let svg = core
                    .render_page_svg_native(heavy)
                    .map_err(|e| anyhow!("{e}"))?;
                r.svg(&svg, 1.25)?;
                total += t.elapsed();
            }
            println!(
                "{label}: edit then redraw page {} (svg + raster), mean of 3: {:?}",
                heavy + 1,
                total / 3
            );
        }
        Ok(())
    }

    #[test]
    fn hairlines_widen_only_sub_pixel_strokes() {
        let svg =
            r#"<line stroke-width="0.48"/><rect stroke-width="1.5"/><path stroke-width="0"/>"#;
        assert_eq!(
            super::hairlines(svg, 0.8),
            r#"<line stroke-width="0.800"/><rect stroke-width="1.5"/><path stroke-width="0"/>"#
        );
        assert_eq!(super::hairlines("no strokes", 1.0), "no strokes");
        assert_eq!(
            super::hairlines(r#"x stroke-width="0.1"#, 1.0),
            r#"x stroke-width="0.1"#
        );
    }
}
