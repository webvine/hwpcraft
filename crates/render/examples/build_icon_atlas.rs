//! Build the ribbon icon atlas from Lucide SVG icons (ISC, assets/icons/lucide):
//! `cargo run -p hwpcraft-render --example build_icon_atlas`.
//!
//! Each 64 px tile holds one icon. Tile order must match `Icon` in
//! crates/ui-egui/src/icons.rs. Tiles whose icon is drawn at runtime
//! (New, Bold, Italic, Underline, Strike, TextColor) still get a tile so the
//! indices stay aligned.
use anyhow::{Context, Result};

/// Lucide icon file name per atlas tile, in `Icon` order.
pub const TILES: &[&str] = &[
    // 파일
    "file-down", // New (drawn at runtime)
    "folder-open",
    "save",
    "save-all",
    "file-down",
    "printer",
    "file-search",
    "info",
    // 편집
    "undo-2",
    "redo-2",
    "scissors",
    "copy",
    "clipboard-paste",
    "square-dashed-mouse-pointer",
    "paintbrush",
    "eraser",
    "search",
    "replace",
    "arrow-down-a-z",
    "mouse-pointer-2",
    "lock",
    // 글자·문단
    "type",
    "pilcrow",
    "brush",
    "type", // Bold (drawn at runtime)
    "type", // Italic (drawn at runtime)
    "type", // Underline (drawn at runtime)
    "type", // Strike (drawn at runtime)
    "subscript",
    "superscript",
    "square",
    "highlighter",
    "type", // TextColor (drawn at runtime)
    "fold-horizontal",
    "unfold-horizontal",
    "align-justify",
    "align-left",
    "align-center",
    "align-right",
    "align-horizontal-distribute-center",
    "align-horizontal-space-between",
    "indent-decrease",
    "indent-increase",
    "unfold-vertical",
    "wrap-text",
    "outdent",
    "a-large-small",
    "list-ordered",
    "list",
    "images",
    "list-tree",
    "arrow-left-to-line",
    "arrow-right-to-line",
    "list-restart",
    "layout-grid",
    // 쪽
    "file-cog",
    "rectangle-vertical",
    "rectangle-horizontal",
    "square-dashed",
    "move-horizontal",
    "layers",
    "panel-top",
    "panel-bottom",
    "frame",
    "hash",
    "list-start",
    "eye-off",
    "rows-3",
    "separator-horizontal",
    "separator-vertical",
    "file-x",
    "columns-2",
    "columns-3",
    "square-split-vertical",
    "tag",
    "grid-3x3",
    // 입력
    "shapes",
    "image",
    "table",
    "rectangle-ellipsis",
    "omega",
    "link",
    "chart-column",
    "sticky-note",
    "sigma",
    "type-outline",
    "package",
    "square-check",
    "braces",
    "message-square-text",
    "book-marked",
    "spell-check",
    "bookmark",
    "locate",
    "chevron-left",
    "chevron-right",
    "languages",
    "book-a",
    // 보기
    "scan",
    "palette",
    "zoom-out",
    "zoom-in",
    "percent",
    "maximize",
    "move-horizontal",
    "square-split-horizontal",
    "app-window",
    "volume-2",
    "eye",
    // 보안·검토·도구
    "shield",
    "file-diff",
    "history",
    "git-compare",
    "check",
    "x",
    "messages-square",
    "message-circle",
    "book-text",
    "table-of-contents",
    "quote",
    "library",
    "terminal",
    "wand-sparkles",
    "book-open",
    "settings-2",
    "user",
    "circle-help",
    // 표
    "between-vertical-start",
    "between-vertical-end",
    "between-horizontal-start",
    "between-horizontal-end",
    "table-rows-split",
    "table-columns-split",
    "combine",
    "split",
    "align-vertical-distribute-center",
    "align-horizontal-distribute-center",
    "square-dashed",
    "table-properties",
    "grid-2x2",
    "calculator",
    "square-sigma",
    "captions",
    "trash-2",
    "scaling",
];
const COLUMNS: u32 = 16;
const TILE: u32 = 64;
/// Lucide icons are 24 px with a 2 px stroke; draw them 48 px inside the tile.
const ICON: f32 = 48.0;
const INK: &str = "#333333";

fn main() -> Result<()> {
    let rows = (TILES.len() as u32).div_ceil(COLUMNS);
    let mut atlas =
        resvg::tiny_skia::Pixmap::new(COLUMNS * TILE, rows * TILE).context("atlas allocation")?;
    let scale = ICON / 24.0;
    let offset = (TILE as f32 - ICON) / 2.0;
    for (i, name) in TILES.iter().enumerate() {
        let path = format!("assets/icons/lucide/{name}.svg");
        let source = std::fs::read_to_string(&path).with_context(|| path.clone())?;
        let source = source.replace("currentColor", INK);
        let tree = resvg::usvg::Tree::from_str(&source, &resvg::usvg::Options::default())
            .with_context(|| path.clone())?;
        let x = (i as u32 % COLUMNS * TILE) as f32 + offset;
        let y = (i as u32 / COLUMNS * TILE) as f32 + offset;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(x, y),
            &mut atlas.as_mut(),
        );
    }
    atlas.save_png("assets/toolbar-icons.png")?;
    println!(
        "{} tiles, {}x{}",
        TILES.len(),
        atlas.width(),
        atlas.height()
    );
    Ok(())
}
