#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use anyhow::{Result, anyhow};
use hwpcraft_doc::DocumentCore;
pub struct Renderer {
    options: resvg::usvg::Options<'static>,
}
pub struct Page {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub svg: String,
}
impl Default for Renderer {
    fn default() -> Self {
        Self {
            options: options(true),
        }
    }
}

/// Parse options with the system fonts and the Korean default family; `share` maps
/// the font files once (see [`share_font_files`]).
fn options(share: bool) -> resvg::usvg::Options<'static> {
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    if share {
        share_font_files(options.fontdb_mut());
    }
    let family = installed_korean_font(&options.fontdb);
    options.fontdb_mut().set_sans_serif_family(family.clone());
    options.fontdb_mut().set_serif_family(family.clone());
    options.font_family = family;
    options
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
        let scale = scale.clamp(0.25, 2.0);
        let screen = hairlines(svg, 1.0 / scale);
        let tree = resvg::usvg::Tree::from_str(&screen, &self.options)?;
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
        Renderer {
            options: options(false),
        }
    }

    fn sample_core() -> anyhow::Result<DocumentCore> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../samples/HwpCraft-시작하기.hwpx");
        hwpcraft_formats::open(&path)
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
