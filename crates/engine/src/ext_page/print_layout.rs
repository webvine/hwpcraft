//! 인쇄 상세: which pages in which order, copies, 모아 찍기 (n-up) and a text watermark,
//! applied to the prepared print SVGs just before they become PDF pages. Nothing here
//! changes the document; the watermark exists only in the printed output.
use anyhow::{Result, bail};
use hwpcraft_doc::PdfLink;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Watermark {
    pub text: String,
    /// 0.05–0.6 (default 0.15).
    #[serde(default)]
    pub opacity: Option<f32>,
}

/// Print options beyond the page range. The default prints every page once, as is.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PrintLayout {
    /// "all" (default), "odd" or "even" — physical page numbers.
    #[serde(default = "all")]
    pub odd_even: String,
    #[serde(default)]
    pub reverse: bool,
    /// Copies written into the file, 1–99 (default 1).
    #[serde(default = "one")]
    pub copies: u32,
    /// With copies: whole set after whole set (true, default) or each page repeated.
    #[serde(default = "yes")]
    pub collate: bool,
    /// Pages per sheet: 1 (default), 2, 4 or 8.
    #[serde(default = "one")]
    pub n_up: u32,
    #[serde(default)]
    pub watermark: Option<Watermark>,
}

fn all() -> String {
    "all".into()
}
fn one() -> u32 {
    1
}
fn yes() -> bool {
    true
}

impl Default for PrintLayout {
    fn default() -> Self {
        Self {
            odd_even: all(),
            reverse: false,
            copies: 1,
            collate: true,
            n_up: 1,
            watermark: None,
        }
    }
}

impl PrintLayout {
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.odd_even.as_str(), "all" | "odd" | "even") {
            bail!("oddEven은 all, odd, even 중 하나입니다.");
        }
        if !(1..=99).contains(&self.copies) {
            bail!("복사본은 1–99부입니다.");
        }
        if !matches!(self.n_up, 1 | 2 | 4 | 8) {
            bail!("모아 찍기는 한 장에 1, 2, 4, 8쪽입니다.");
        }
        if let Some(w) = &self.watermark {
            if w.text.trim().is_empty() || w.text.chars().count() > 40 {
                bail!("워터마크 글은 1–40자입니다.");
            }
            if w.opacity.is_some_and(|o| !(0.05..=0.6).contains(&o)) {
                bail!("워터마크 진하기는 0.05–0.6입니다.");
            }
        }
        Ok(())
    }

    /// Indexes into the selected pages (0-based physical `pages`) in print order:
    /// odd/even filter, reverse, then copies.
    pub fn order(&self, pages: &[u32]) -> Vec<usize> {
        let mut picked: Vec<usize> = (0..pages.len())
            .filter(|i| match self.odd_even.as_str() {
                "odd" => pages[*i].is_multiple_of(2),
                "even" => !pages[*i].is_multiple_of(2),
                _ => true,
            })
            .collect();
        if self.reverse {
            picked.reverse();
        }
        let copies = self.copies.max(1) as usize;
        if self.collate {
            (0..copies).flat_map(|_| picked.iter().copied()).collect()
        } else {
            picked
                .iter()
                .flat_map(|i| std::iter::repeat_n(*i, copies))
                .collect()
        }
    }

    /// Sheets the selected `pages` print on, one copy (미리 보기 pages through these).
    pub fn sheet_count(&self, pages: &[u32]) -> usize {
        self.one_copy()
            .order(pages)
            .len()
            .div_ceil(self.n_up.max(1) as usize)
    }

    /// The physical pages printed on sheet `sheet` (0-based) of one copy.
    pub fn sheet_pages(&self, pages: &[u32], sheet: usize) -> Vec<u32> {
        let n = self.n_up.max(1) as usize;
        self.one_copy()
            .order(pages)
            .into_iter()
            .skip(sheet * n)
            .take(n)
            .map(|i| pages[i])
            .collect()
    }

    fn one_copy(&self) -> Self {
        Self {
            copies: 1,
            ..self.clone()
        }
    }

    /// One sheet from the print SVGs of its pages (from [`PrintLayout::sheet_pages`]):
    /// watermark on each page, then 모아 찍기.
    pub fn compose_sheet(&self, svgs: Vec<String>) -> Result<String> {
        self.validate()?;
        if svgs.is_empty() {
            bail!("미리 볼 쪽이 없습니다.");
        }
        let mut svgs = svgs;
        if let Some(w) = &self.watermark {
            for svg in &mut svgs {
                *svg = with_watermark(svg, w)?;
            }
        }
        if self.n_up == 1 {
            return Ok(svgs.swap_remove(0));
        }
        let group: Vec<usize> = (0..svgs.len()).collect();
        let links = vec![Vec::new(); svgs.len()];
        Ok(n_up_sheet(&group, &svgs, &links, self.n_up)?.0)
    }

    /// Apply the layout to prepared print SVGs (one per selected page, in `pages`
    /// order) and their links; returns the sheets to convert.
    pub fn apply(
        &self,
        pages: &[u32],
        svgs: Vec<String>,
        links: Vec<Vec<PdfLink>>,
    ) -> Result<(Vec<String>, Vec<Vec<PdfLink>>)> {
        self.validate()?;
        let order = self.order(pages);
        if order.is_empty() {
            bail!(
                "고른 쪽 가운데 {} 쪽이 없습니다.",
                match self.odd_even.as_str() {
                    "odd" => "홀수",
                    _ => "짝수",
                }
            );
        }
        let mut svgs: Vec<String> = svgs;
        if let Some(w) = &self.watermark {
            for svg in &mut svgs {
                *svg = with_watermark(svg, w)?;
            }
        }
        let links = retarget(&order, self.n_up, links);
        let sheets: Vec<(String, Vec<PdfLink>)> = if self.n_up == 1 {
            order
                .iter()
                .map(|&i| (svgs[i].clone(), links[i].clone()))
                .collect()
        } else {
            order
                .chunks(self.n_up as usize)
                .map(|group| n_up_sheet(group, &svgs, &links, self.n_up))
                .collect::<Result<_>>()?
        };
        Ok(sheets.into_iter().unzip())
    }
}

/// Links inside the document name their target by its index in the prepared pages;
/// on one page per sheet that becomes the sheet of the target's first print. 모아
/// 찍기 sheets keep web links only (a page's place on a sheet is not a target yet).
fn retarget(order: &[usize], n_up: u32, mut links: Vec<Vec<PdfLink>>) -> Vec<Vec<PdfLink>> {
    for link in links.iter_mut().flatten() {
        if let Some(dest) = link.dest.as_mut() {
            match order
                .iter()
                .position(|&i| i == dest.page)
                .filter(|_| n_up == 1)
            {
                Some(sheet) => dest.page = sheet,
                None => link.dest = None,
            }
        }
    }
    links
}

/// Width and height of a page SVG from its root `width`/`height` attributes.
fn svg_size(svg: &str) -> Result<(f64, f64)> {
    let open = root_tag(svg)?;
    let attr = |name: &str| -> Option<f64> {
        let key = format!(" {name}=\"");
        let start = open.find(&key)? + key.len();
        open[start..]
            .split('"')
            .next()?
            .trim_end_matches("px")
            .parse()
            .ok()
    };
    match (attr("width"), attr("height")) {
        (Some(w), Some(h)) if w > 0.0 && h > 0.0 => Ok((w, h)),
        _ => bail!("쪽 그림의 크기를 읽지 못했습니다."),
    }
}

fn root_tag(svg: &str) -> Result<&str> {
    let start = svg
        .find("<svg")
        .ok_or_else(|| anyhow::anyhow!("쪽 그림이 SVG가 아닙니다."))?;
    let end = svg[start..]
        .find('>')
        .ok_or_else(|| anyhow::anyhow!("쪽 그림이 SVG가 아닙니다."))?;
    Ok(&svg[start..start + end + 1])
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The page with a light diagonal text across its middle, drawn above the content.
pub fn with_watermark(svg: &str, mark: &Watermark) -> Result<String> {
    let (w, h) = svg_size(svg)?;
    let end = svg
        .rfind("</svg>")
        .ok_or_else(|| anyhow::anyhow!("쪽 그림이 SVG가 아닙니다."))?;
    let chars = mark.text.chars().count().max(1) as f64;
    // Fill about 70 % of the diagonal.
    let size = ((w * w + h * h).sqrt() * 0.7 / chars).clamp(18.0, 160.0);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let angle = -(h / w).atan().to_degrees();
    let overlay = format!(
        "<g data-hwpcraft-watermark=\"true\" pointer-events=\"none\"><text x=\"{cx:.2}\" y=\"{cy:.2}\" \
         transform=\"rotate({angle:.2} {cx:.2} {cy:.2})\" text-anchor=\"middle\" dominant-baseline=\"middle\" \
         font-family=\"Malgun Gothic, 맑은 고딕, Noto Sans CJK KR, sans-serif\" font-weight=\"bold\" \
         font-size=\"{size:.2}\" fill=\"#000000\" fill-opacity=\"{:.2}\">{}</text></g>\n",
        mark.opacity.unwrap_or(0.15),
        escape(&mark.text)
    );
    let mut out = String::with_capacity(svg.len() + overlay.len());
    out.push_str(&svg[..end]);
    out.push_str(&overlay);
    out.push_str(&svg[end..]);
    Ok(out)
}

/// Grid of an n-up sheet: (columns, rows, sheet turned 90° from the page).
fn grid(n_up: u32) -> (usize, usize, bool) {
    match n_up {
        2 => (2, 1, true),
        4 => (2, 2, false),
        _ => (4, 2, true),
    }
}

/// Ids made unique per page so clip paths and gradients of different pages on one
/// sheet do not collide.
fn prefix_ids(svg: &str, prefix: &str) -> String {
    svg.replace(" id=\"", &format!(" id=\"{prefix}"))
        .replace("url(#", &format!("url(#{prefix}"))
        .replace("href=\"#", &format!("href=\"#{prefix}"))
}

/// One sheet holding the pages `group` (indexes into `svgs`), scaled into a grid on a
/// sheet the size of the first page (turned 90° for 2 and 8 pages, so landscape pages
/// stack on a portrait sheet); links move with them.
fn n_up_sheet(
    group: &[usize],
    svgs: &[String],
    links: &[Vec<PdfLink>],
    n_up: u32,
) -> Result<(String, Vec<PdfLink>)> {
    let (pw, ph) = svg_size(&svgs[group[0]])?;
    let (cols, rows, turned) = grid(n_up);
    let (sw, sh) = if turned { (ph, pw) } else { (pw, ph) };
    // The grid's longer run lies along the sheet's longer side.
    let (cols, rows) = if (sw >= sh) == (cols >= rows) {
        (cols, rows)
    } else {
        (rows, cols)
    };
    let margin = 12.0;
    let gap = 8.0;
    let cell_w = (sw - 2.0 * margin - gap * (cols as f64 - 1.0)) / cols as f64;
    let cell_h = (sh - 2.0 * margin - gap * (rows as f64 - 1.0)) / rows as f64;
    let mut body = String::new();
    let mut sheet_links = Vec::new();
    for (slot, &index) in group.iter().enumerate() {
        let svg = &svgs[index];
        let (w, h) = svg_size(svg)?;
        let scale = (cell_w / w).min(cell_h / h);
        let (col, row) = (slot % cols, slot / cols);
        let x = margin + col as f64 * (cell_w + gap) + (cell_w - w * scale) / 2.0;
        let y = margin + row as f64 * (cell_h + gap) + (cell_h - h * scale) / 2.0;
        let open = root_tag(svg)?;
        let inner_start = svg.find(open).unwrap_or(0) + open.len();
        let inner_end = svg.rfind("</svg>").unwrap_or(svg.len());
        let inner = prefix_ids(&svg[inner_start..inner_end], &format!("n{slot}-"));
        body.push_str(&format!(
            "<svg x=\"{x:.3}\" y=\"{y:.3}\" width=\"{:.3}\" height=\"{:.3}\" viewBox=\"0 0 {w} {h}\" overflow=\"hidden\">\
             <rect width=\"{w}\" height=\"{h}\" fill=\"#ffffff\"/>{inner}</svg>\
             <rect x=\"{x:.3}\" y=\"{y:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"none\" stroke=\"#bbbbbb\" stroke-width=\"0.5\"/>\n",
            w * scale,
            h * scale,
            w * scale,
            h * scale,
        ));
        for link in &links[index] {
            let mut moved = link.clone();
            moved.rect.x = x + link.rect.x * scale;
            moved.rect.y = y + link.rect.y * scale;
            moved.rect.width = link.rect.width * scale;
            moved.rect.height = link.rect.height * scale;
            sheet_links.push(moved);
        }
    }
    let sheet = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
         width=\"{sw}\" height=\"{sh}\" viewBox=\"0 0 {sw} {sh}\">\n<rect width=\"{sw}\" height=\"{sh}\" fill=\"#ffffff\"/>\n{body}</svg>\n"
    );
    Ok((sheet, sheet_links))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(n: u32) -> String {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"793.7\" height=\"1122.5\" viewBox=\"0 0 793.7 1122.5\">\
             <defs><clipPath id=\"body-clip-3\"><rect width=\"10\" height=\"10\"/></clipPath></defs>\
             <g clip-path=\"url(#body-clip-3)\"><text>{n}</text></g></svg>"
        )
    }

    #[test]
    fn order_filters_reverses_and_repeats() {
        let pages = [0, 1, 2, 3, 4];
        let l = |f: fn(&mut PrintLayout)| {
            let mut l = PrintLayout::default();
            f(&mut l);
            l.order(&pages)
        };
        assert_eq!(l(|_| {}), vec![0, 1, 2, 3, 4]);
        assert_eq!(l(|l| l.odd_even = "odd".into()), vec![0, 2, 4]);
        assert_eq!(l(|l| l.odd_even = "even".into()), vec![1, 3]);
        assert_eq!(l(|l| l.reverse = true), vec![4, 3, 2, 1, 0]);
        assert_eq!(
            l(|l| {
                l.odd_even = "even".into();
                l.copies = 2;
            }),
            vec![1, 3, 1, 3]
        );
        assert_eq!(
            l(|l| {
                l.odd_even = "even".into();
                l.copies = 2;
                l.collate = false;
            }),
            vec![1, 1, 3, 3]
        );
    }

    #[test]
    fn bad_options_are_refused() -> Result<()> {
        for json in [
            r#"{"oddEven":"left"}"#,
            r#"{"copies":0}"#,
            r#"{"nUp":3}"#,
            r#"{"watermark":{"text":" "}}"#,
            r#"{"watermark":{"text":"x","opacity":0.9}}"#,
        ] {
            let l: PrintLayout = serde_json::from_str(json)?;
            assert!(l.validate().is_err(), "{json}");
        }
        assert!(serde_json::from_str::<PrintLayout>(r#"{"bogus":1}"#).is_err());
        Ok(())
    }

    #[test]
    fn watermark_is_escaped_centered_and_rotated() -> Result<()> {
        let svg = with_watermark(
            &page(1),
            &Watermark {
                text: "대외비 <초안>".into(),
                opacity: None,
            },
        )?;
        assert!(svg.contains("&lt;초안&gt;") && svg.contains("fill-opacity=\"0.15\""));
        assert!(svg.contains("x=\"396.85\" y=\"561.25\""));
        assert!(svg.trim_end().ends_with("</svg>"));
        Ok(())
    }

    #[test]
    fn four_up_makes_one_portrait_sheet_with_unique_ids() -> Result<()> {
        let layout = PrintLayout {
            n_up: 4,
            ..Default::default()
        };
        let pages = [0, 1, 2, 3, 4];
        let svgs = pages.iter().map(|n| page(*n)).collect();
        let link = PdfLink {
            uri: "https://example.com".into(),
            ..Default::default()
        };
        let links = vec![vec![], vec![link], vec![], vec![], vec![]];
        let (sheets, sheet_links) = layout.apply(&pages, svgs, links)?;
        assert_eq!(sheets.len(), 2, "five pages on two sheets");
        let first = &sheets[0];
        assert_eq!(svg_size(first)?, (793.7, 1122.5));
        for slot in 0..4 {
            assert!(first.contains(&format!("id=\"n{slot}-body-clip-3\"")));
            assert!(first.contains(&format!("url(#n{slot}-body-clip-3)")));
        }
        assert_eq!(sheet_links[0].len(), 1, "the link moved onto the sheet");
        assert!(sheet_links[0][0].rect.x > 12.0);
        // Two pages per sheet turn the sheet to landscape.
        let two = PrintLayout {
            n_up: 2,
            ..Default::default()
        };
        let (sheets, _) = two.apply(&[0, 1], vec![page(0), page(1)], vec![vec![], vec![]])?;
        assert_eq!(svg_size(&sheets[0])?, (1122.5, 793.7));
        // Landscape pages: a portrait sheet, one page above the other.
        let wide = |n: u32| {
            page(n).replace(
                "width=\"793.7\" height=\"1122.5\" viewBox=\"0 0 793.7 1122.5\"",
                "width=\"1122.5\" height=\"793.7\" viewBox=\"0 0 1122.5 793.7\"",
            )
        };
        let (sheets, _) = two.apply(&[0, 1], vec![wide(0), wide(1)], vec![vec![], vec![]])?;
        assert_eq!(svg_size(&sheets[0])?, (793.7, 1122.5));
        let xs: Vec<&str> = sheets[0]
            .match_indices("<svg x=\"")
            .map(|(i, _)| sheets[0][i + 8..].split('"').next().unwrap_or_default())
            .collect();
        assert_eq!(xs.len(), 2);
        assert_eq!(xs[0], xs[1], "stacked, not side by side");
        Ok(())
    }
}
