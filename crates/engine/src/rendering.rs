//! HwpCraft output additions shared by desktop, CLI, MCP and PDF printing.
use super::*;
use hwpcraft_doc::RenderProfile;

fn with_highlights(mut svg: String, page: u32, rects: &[HighlightRect]) -> Result<String> {
    let end = svg
        .rfind("</svg>")
        .ok_or_else(|| anyhow!("쪽 SVG가 올바르지 않습니다."))?;
    let mut overlay = String::new();
    for rect in rects.iter().filter(|r| r.page == page) {
        let color = rect.color.strip_prefix('#').unwrap_or("");
        if color.len() != 6 || !color.bytes().all(|ch| ch.is_ascii_hexdigit()) {
            continue;
        }
        if ![rect.x, rect.y, rect.width, rect.height]
            .iter()
            .all(|x| x.is_finite())
            || rect.width <= 0.0
            || rect.height <= 0.0
        {
            continue;
        }
        // Match the former desktop-only alpha (110/255). Append above text so
        // screen and exported output use exactly the same compositing order.
        overlay.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#{}\" fill-opacity=\"0.43137255\"/>\n",
            rect.x, rect.y, rect.width, rect.height, color
        ));
    }
    if !overlay.is_empty() {
        svg.insert_str(
            end,
            &format!("<g data-hwpcraft-markpen=\"true\" pointer-events=\"none\">{overlay}</g>\n"),
        );
    }
    Ok(svg)
}

impl Session {
    /// A page in screen coordinates, including HwpCraft-owned drawing additions.
    pub fn render_page_svg(&self, page: u32) -> Result<String> {
        let fields = self.page_field_layout()?;
        let svg = match fields.as_ref() {
            Some(layout) => layout.render_svg(page, RenderProfile::Screen)?,
            None => self.core.render_page_svg_native(page).map_err(err)?,
        };
        let core = fields.as_ref().map_or(&self.core, |layout| &layout.core);
        let svg = chart_creation::with_chart_overrides(core, page, RenderProfile::Screen, svg)?;
        with_highlights(svg, page, &self.output_highlight_rects())
    }

    /// Vector PDF output with Print-profile filtering and clickable links intact.
    pub fn render_pdf(&self, pages: &[u32]) -> Result<Vec<u8>> {
        let options = hwpcraft_formats::pdf_options();
        let fields = self.page_field_layout()?;
        let core = fields.as_ref().map_or(&self.core, |layout| &layout.core);
        let _release = ReleaseRenderCaches([&self.core, core]);
        let rects = self.output_highlight_rects();
        let mut has_chart_overrides = false;
        for &page in pages {
            if chart_creation::page_has_chart_overrides(core, page, RenderProfile::Print)? {
                has_chart_overrides = true;
                break;
            }
        }
        if rects.is_empty() && fields.is_none() && !has_chart_overrides {
            return core
                .render_pages_pdf_native_with_options(pages, &options)
                .map_err(err);
        }
        self.pdf_job(pages)?.convert()
    }

    /// The session-side half of PDF export: every page's print SVG (page fields,
    /// chart overrides and highlights applied) and its hyperlinks. This is fast
    /// (about 0.6 s for 217 pages); the returned job's `convert` is the slow part
    /// (RHWP font layout and embedding, minutes on long documents) and needs no
    /// session, so the desktop runs it on a worker thread to keep the window live.
    pub fn pdf_job(&self, pages: &[u32]) -> Result<PdfJob> {
        let mut prep = self.pdf_prepare(pages)?;
        self.pdf_prepare_step(&mut prep, std::time::Duration::MAX)?;
        self.pdf_finish(prep)
    }

    /// Start preparing `pages` a few at a time (see [`Session::pdf_prepare_step`]),
    /// so a long document does not hold the window for seconds.
    pub fn pdf_prepare(&self, pages: &[u32]) -> Result<PdfPrep> {
        if pages.is_empty() {
            bail!("PDF로 내보낼 쪽이 없습니다.");
        }
        Ok(PdfPrep {
            pages: pages.to_vec(),
            generation: self.generation,
            svgs: Vec::with_capacity(pages.len()),
            links: Vec::with_capacity(pages.len()),
        })
    }

    /// Build pages until `budget` has passed (at least one per call); `true` once
    /// every page is ready. Refuses when the document changed since
    /// [`Session::pdf_prepare`], so one PDF never mixes two versions.
    pub fn pdf_prepare_step(
        &self,
        prep: &mut PdfPrep,
        budget: std::time::Duration,
    ) -> Result<bool> {
        if prep.generation != self.generation {
            bail!("PDF를 준비하는 동안 문서가 바뀌었습니다.");
        }
        let started = std::time::Instant::now();
        let fields = self.page_field_layout()?;
        let core = fields.as_ref().map_or(&self.core, |layout| &layout.core);
        let _release = ReleaseRenderCaches([&self.core, core]);
        let rects = self.output_highlight_rects();
        while let Some(&page) = prep.pages.get(prep.svgs.len()) {
            let svg = match fields.as_ref() {
                Some(layout) => layout.render_svg(page, RenderProfile::Print)?,
                None => core
                    .render_page_svg_layer_with_profile_native(page, RenderProfile::Print)
                    .map_err(err)?,
            };
            let svg = chart_creation::with_chart_overrides(core, page, RenderProfile::Print, svg)?;
            prep.svgs.push(with_highlights(svg, page, &rects)?);
            prep.links.push(
                core.page_hyperlinks_native(page, RenderProfile::Print)
                    .map_err(err)?
                    .iter()
                    .map(|link| link.pdf_link())
                    .collect::<Vec<_>>(),
            );
            if started.elapsed() >= budget {
                break;
            }
        }
        Ok(prep.svgs.len() == prep.pages.len())
    }

    /// Turn a fully prepared [`PdfPrep`] into the job that converts it.
    pub fn pdf_finish(&self, prep: PdfPrep) -> Result<PdfJob> {
        if prep.generation != self.generation {
            bail!("PDF를 준비하는 동안 문서가 바뀌었습니다.");
        }
        if prep.svgs.len() != prep.pages.len() {
            bail!("PDF 쪽 준비가 끝나지 않았습니다.");
        }
        let options = hwpcraft_formats::pdf_options();
        let PdfPrep {
            pages, svgs, links, ..
        } = prep;
        Ok(PdfJob {
            pages: pages.len(),
            convert: Box::new(move || {
                hwpcraft_doc::svgs_to_pdf_with_links(&svgs, &links, &options)
                    .map_err(anyhow::Error::msg)
            }),
        })
    }
}

/// PDF pages prepared incrementally; bound to the document generation it started at.
pub struct PdfPrep {
    pages: Vec<u32>,
    generation: u64,
    svgs: Vec<String>,
    links: Vec<Vec<hwpcraft_doc::PdfLink>>,
}

impl PdfPrep {
    pub fn total(&self) -> usize {
        self.pages.len()
    }
    pub fn ready(&self) -> usize {
        self.svgs.len()
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

/// Building every page for PDF leaves a render tree per page in RHWP's caches (about
/// 640 MB for 381 pages). Drop them when the export finishes or fails; the next screen
/// render rebuilds only the pages it asks for.
struct ReleaseRenderCaches<'a>([&'a hwpcraft_doc::DocumentCore; 2]);

impl Drop for ReleaseRenderCaches<'_> {
    fn drop(&mut self) {
        for core in self.0 {
            core.release_page_render_caches();
        }
    }
}

/// Prepared PDF pages; `convert` may run on any thread (see `Session::pdf_job`).
pub struct PdfJob {
    pages: usize,
    convert: Box<dyn FnOnce() -> Result<Vec<u8>> + Send>,
}
impl PdfJob {
    pub fn page_count(&self) -> usize {
        self.pages
    }
    /// Lay out, embed fonts and assemble the PDF bytes.
    pub fn convert(self) -> Result<Vec<u8>> {
        (self.convert)()
    }
}
