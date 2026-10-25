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
        let mut options = resvg::usvg::Options::default();
        options.fontdb_mut().load_system_fonts();
        let family = installed_korean_font(&options.fontdb).to_owned();
        options.fontdb_mut().set_sans_serif_family(family.clone());
        options.fontdb_mut().set_serif_family(family.clone());
        options.font_family = family;
        Self { options }
    }
}

/// First installed family from the OS candidate list, else the first candidate.
fn installed_korean_font(db: &resvg::usvg::fontdb::Database) -> &'static str {
    let candidates = hwpcraft_doc::fonts::KOREAN_CANDIDATES;
    candidates
        .iter()
        .copied()
        .find(|name| {
            db.faces()
                .any(|face| face.families.iter().any(|(n, _)| n == name))
        })
        .unwrap_or(candidates[0])
}

/// The Korean font family HwpCraft uses on this computer (probed once).
pub fn default_korean_font() -> &'static str {
    static FONT: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        installed_korean_font(&db)
    })
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
        .filter(|face| face.families.iter().any(|(n, _)| n == family))
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
