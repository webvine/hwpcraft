#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
/// User-facing x.xx release number, shared by desktop, CLI and MCP.
pub const VERSION: &str = include_str!("../../../VERSION");
pub use rhwp::DocumentCore;
pub use rhwp::docdiff;
pub use rhwp::document_core::hyperlink::{HyperlinkInfo, HyperlinkTarget};
/// Privacy scan option/result types for `DocumentCore::scan_pii` / `detect_hidden_text`.
pub use rhwp::document_core::queries::{hidden_text, pii_scan};
pub use rhwp::model;
pub use rhwp::paint::RenderProfile;
pub use rhwp::paint::{LayerNode, LayerNodeKind, PaintOp};
pub use rhwp::parser::body_text::parse_paragraph_list as parse_hwp_paragraph_list;
pub use rhwp::parser::record::Record as HwpRecord;
pub use rhwp::password_crypto;
pub use rhwp::renderer::hyperlinks::PdfLink;
pub use rhwp::renderer::layout::set_layout_diagnostics;
pub use rhwp::renderer::pdf::PdfExportOptions;
pub use rhwp::renderer::pdf::svgs_to_pdf_with_links;
pub use rhwp::renderer::render_tree::{PageRenderTree, RenderNode, RenderNodeType};
pub use rhwp::renderer::svg::SvgRenderer;
pub use rhwp::renderer::{
    layer_renderer::LayerRenderer, svg::FontEmbedMode, svg_layer::SvgLayerRenderer,
};
pub use rhwp::serializer::record_writer::write_records as write_hwp_records;
/// RHWP view-flag setters (paragraph marks, control codes, transparent borders)
/// live on this wrapper; it derefs to `DocumentCore` and compiles natively.
pub use rhwp::wasm_api::HwpDocument;

/// Public chart/compound-file primitives used by HwpCraft's own chart authoring.
pub mod chart_io {
    pub use rhwp::ooxml_chart::{
        OoxmlChart, OoxmlChartType, ScatterStyle, data::scan_chart_values,
    };
    pub use rhwp::parser::ole_container::{all_ole_streams, ole_root_clsid};
    pub use rhwp::serializer::mini_cfb::build_cfb_with_root_clsid;
    pub use rhwp::serializer::ole_container::replace_ole_stream;
}
use serde::{Deserialize, Serialize};

/// Korean fonts that ship with each operating system. Callers that can probe
/// installed fonts (`hwpcraft-render`) pick the first installed candidate; callers
/// that cannot (PDF export fallbacks) use `default_family()`.
pub mod fonts {
    #[cfg(target_os = "windows")]
    pub const KOREAN_CANDIDATES: &[&str] = &["맑은 고딕", "Malgun Gothic", "굴림", "Gulim"];
    #[cfg(target_os = "macos")]
    pub const KOREAN_CANDIDATES: &[&str] = &[
        "Apple SD Gothic Neo",
        "AppleGothic",
        "Noto Sans CJK KR",
        "NanumGothic",
    ];
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    pub const KOREAN_CANDIDATES: &[&str] = &[
        "Noto Sans CJK KR",
        "Noto Sans KR",
        "NanumGothic",
        "NanumBarunGothic",
        "UnDotum",
        "Baekmuk Dotum",
    ];
    /// The family name to request when nothing can be probed (PDF fallbacks).
    /// RHWP matches by name, so the English name is used where the OS has one.
    pub fn default_family() -> &'static str {
        if cfg!(target_os = "windows") {
            "Malgun Gothic"
        } else {
            KOREAN_CANDIDATES[0]
        }
    }
}

/// HwpCraft text offsets are Unicode scalar positions, not UTF-8 byte offsets.
/// The engine adapts RHWP split APIs, which also count inline controls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub section: usize,
    pub paragraph: usize,
    pub offset: usize,
    pub cell: Option<Cell>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub control: usize,
    pub index: usize,
    pub paragraph: usize,
}
