//! Ribbon icons: Lucide icon atlas (ISC, assets/toolbar-icons.png, built by
//! crates/render/examples/build_icon_atlas.rs) plus a few locally drawn symbols.
//! Large buttons are 76 px tall with a 32 px icon, small ones 24 px.
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, TextureHandle, Ui, Vec2};

/// Atlas tiles, in the order of `TILES` in crates/render/examples/build_icon_atlas.rs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(usize)]
#[allow(dead_code)] // Every tile keeps its slot so later tiles stay aligned with the atlas.
pub enum Icon {
    // 파일
    New,
    Open,
    Save,
    SaveAs,
    Pdf,
    Print,
    Preview,
    DocInfo,
    // 편집
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    FormatCopy,
    CtrlMarkDelete,
    Find,
    Replace,
    Sort,
    ObjectSelect,
    ObjectProtect,
    // 글자·문단
    Character,
    Paragraph,
    Style,
    Bold,
    Italic,
    Underline,
    Strike,
    Subscript,
    Superscript,
    CharBorder,
    Highlight,
    TextColor,
    SpacingNarrow,
    SpacingWide,
    Justify,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Distribute,
    SplitAlign,
    MarginDecrease,
    MarginIncrease,
    LineSpacing,
    IndentFirst,
    OutdentFirst,
    DropCap,
    Numbering,
    Bullet,
    PictureBullet,
    Outline,
    LevelUp,
    LevelDown,
    NumberRestart,
    StyleGallery,
    // 쪽
    PageSetup,
    Portrait,
    Landscape,
    PageMargins,
    TextDirection,
    MasterPage,
    Header,
    Footer,
    PageBorder,
    PageNumber,
    NewNumber,
    HidePage,
    LineNumber,
    PageBreak,
    ColumnBreak,
    PageDelete,
    Columns,
    ColumnDefBreak,
    Section,
    Label,
    Grid,
    // 입력
    Shape,
    Image,
    Table,
    Textbox,
    Symbol,
    Link,
    Chart,
    Memo,
    Equation,
    TextArt,
    Ole,
    FormObject,
    Field,
    Footnote,
    Endnote,
    ProofMark,
    Bookmark,
    CrossRef,
    Prev,
    Next,
    Translate,
    Hanja,
    // 보기
    PageOutline,
    ViewColor,
    ZoomOut,
    ZoomIn,
    Zoom100,
    FitPage,
    FitWidth,
    SplitView,
    Windows,
    Pronounce,
    Eye,
    // 보안·검토·도구
    Privacy,
    TrackChanges,
    History,
    Compare,
    Accept,
    Reject,
    MemoList,
    Balloons,
    Index,
    Toc,
    Citation,
    Bibliography,
    Macro,
    AutoCorrect,
    Dictionary,
    Customize,
    UserInfo,
    Help,
    // 표
    RowAbove,
    RowBelow,
    ColumnLeft,
    ColumnRight,
    DeleteRow,
    DeleteColumn,
    Merge,
    SplitCell,
    EqualHeight,
    EqualWidth,
    CellBlock,
    TableProperties,
    CellBorder,
    Formula,
    BlockCalc,
    Caption,
    DeleteTable,
    FitTable,
    // Drawn locally (no atlas tile).
    Lock = 1000,
    Rectangle,
    Ellipse,
    Line,
    Arc,
    Polygon,
    Curve,
    Freeline,
    VTextbox,
    Connector,
}

const TILES: usize = Icon::FitTable as usize + 1;
const COLUMNS: usize = 16;
const TILE: f32 = 64.0;
pub const LARGE_HEIGHT: f32 = 76.0;
pub const SMALL: f32 = 24.0;
const INK: Color32 = Color32::from_gray(51);
const HOVER_FILL: Color32 = Color32::from_rgb(229, 241, 251);
const HOVER_LINE: Color32 = Color32::from_rgb(160, 200, 236);
const PRESSED_FILL: Color32 = Color32::from_rgb(222, 222, 222);

pub struct Icons {
    texture: Option<TextureHandle>,
    size: Vec2,
}
impl Icons {
    pub fn new(ctx: &egui::Context) -> Self {
        let image = image::load_from_memory(include_bytes!("../../../assets/toolbar-icons.png"))
            .ok()
            .map(|i| i.to_rgba8());
        let size = image
            .as_ref()
            .map(|i| Vec2::new(i.width() as f32, i.height() as f32))
            .unwrap_or(Vec2::splat(1.0));
        let texture = image.map(|image| {
            ctx.load_texture(
                "toolbar-icons",
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            )
        });
        Self { texture, size }
    }
    pub fn paint(&self, ui: &Ui, rect: Rect, icon: Icon, enabled: bool) {
        let index = icon as usize;
        if index >= TILES {
            draw_local(ui, rect, icon, enabled);
            return;
        }
        if matches!(icon, Icon::New) {
            draw_new(ui, rect, enabled);
            return;
        }
        if draw_glyph(ui, rect, icon, enabled) {
            return;
        }
        let Some(texture) = &self.texture else {
            return;
        };
        let min = Pos2::new(
            (index % COLUMNS) as f32 * TILE,
            (index / COLUMNS) as f32 * TILE,
        );
        let uv = Rect::from_min_size(
            Pos2::new(min.x / self.size.x, min.y / self.size.y),
            Vec2::new(TILE / self.size.x, TILE / self.size.y),
        );
        let tint = if enabled {
            Color32::WHITE
        } else {
            Color32::from_white_alpha(70)
        };
        ui.painter().image(texture.id(), rect, uv, tint);
    }

    /// Hangul's large ribbon button: 32 px icon over one or two label lines
    /// (`\n` splits them); `menu` adds the ∨ drop-down mark after the label.
    /// The caller adds the tooltip.
    pub fn large(
        &self,
        ui: &mut Ui,
        icon: Icon,
        label: &str,
        enabled: bool,
        menu: bool,
        selected: bool,
    ) -> Response {
        let font = egui::FontId::proportional(12.0);
        let lines: Vec<&str> = label.split('\n').collect();
        let widest = lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let mark = if menu && i + 1 == lines.len() {
                    10.0
                } else {
                    0.0
                };
                text_width(ui, line, &font) + mark
            })
            .fold(0.0_f32, f32::max);
        let width = (widest + 12.0).max(38.0).round();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, LARGE_HEIGHT), sense(enabled));
        frame(ui, rect, &response, enabled, selected);
        let image = Rect::from_center_size(
            Pos2::new(rect.center().x, rect.top() + 22.0),
            Vec2::splat(32.0),
        );
        self.paint(ui, image, icon, enabled);
        let color = text_color(enabled);
        // A one-line label sits on the first text row; ∨ then drops to the second.
        let single_menu = menu && lines.len() == 1;
        for (i, line) in lines.iter().enumerate() {
            let y = rect.top() + 47.0 + i as f32 * 16.0;
            let mark = menu && !single_menu && i + 1 == lines.len();
            let w = text_width(ui, line, &font) + if mark { 10.0 } else { 0.0 };
            let x = rect.center().x - w / 2.0;
            ui.painter().text(
                Pos2::new(x, y),
                egui::Align2::LEFT_TOP,
                *line,
                font.clone(),
                color,
            );
            if mark {
                chevron(ui, Pos2::new(x + w - 4.0, y + 8.0), color);
            }
        }
        if single_menu {
            chevron(
                ui,
                Pos2::new(rect.center().x, rect.top() + 47.0 + 16.0 + 8.0),
                color,
            );
        }
        response
    }

    /// Hangul's small 24 px button: icon, optional label and optional ∨ part, with
    /// `tooltip` as given.
    #[allow(clippy::too_many_arguments)]
    pub fn small(
        &self,
        ui: &mut Ui,
        icon: Icon,
        label: Option<&str>,
        tooltip: &str,
        enabled: bool,
        menu: bool,
        selected: bool,
    ) -> Response {
        let font = egui::FontId::proportional(12.0);
        let label_width = label.map(|l| text_width(ui, l, &font) + 6.0).unwrap_or(0.0);
        let width = 23.0 + label_width + if menu { 16.0 } else { 0.0 };
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, SMALL), sense(enabled));
        frame(ui, rect, &response, enabled, selected);
        let image = Rect::from_center_size(
            Pos2::new(rect.left() + 11.5, rect.center().y),
            // RHWP glyphs fill ~90 % of a tile; 18 px gives Hangul's 16 px marks.
            Vec2::splat(18.0),
        );
        self.paint(ui, image, icon, enabled);
        let color = text_color(enabled);
        if let Some(label) = label {
            ui.painter().text(
                Pos2::new(rect.left() + 24.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                font,
                color,
            );
        }
        if menu {
            chevron(ui, Pos2::new(rect.right() - 8.0, rect.center().y), color);
        }
        response.on_hover_text(tooltip)
    }
}

fn sense(enabled: bool) -> Sense {
    if enabled {
        Sense::click()
    } else {
        Sense::hover()
    }
}
fn text_width(ui: &Ui, text: &str, font: &egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), font.clone(), INK)
        .size()
        .x
}
fn text_color(enabled: bool) -> Color32 {
    if enabled {
        INK
    } else {
        Color32::from_gray(172)
    }
}
fn frame(ui: &Ui, rect: Rect, response: &Response, enabled: bool, selected: bool) {
    if selected {
        ui.painter().rect_filled(rect, 2.0, PRESSED_FILL);
    } else if enabled && response.hovered() {
        ui.painter().rect_filled(rect, 2.0, HOVER_FILL);
        ui.painter().rect_stroke(
            rect,
            2.0,
            Stroke::new(1.0, HOVER_LINE),
            egui::StrokeKind::Inside,
        );
    }
}
/// The small ∨ that marks a drop-down part.
pub fn chevron(ui: &Ui, center: Pos2, color: Color32) {
    let s = 3.0;
    ui.painter().line_segment(
        [
            center + Vec2::new(-s, -s / 2.0),
            center + Vec2::new(0.0, s / 2.0),
        ],
        Stroke::new(1.0, color),
    );
    ui.painter().line_segment(
        [
            center + Vec2::new(0.0, s / 2.0),
            center + Vec2::new(s, -s / 2.0),
        ],
        Stroke::new(1.0, color),
    );
}
fn draw_new(ui: &Ui, rect: Rect, enabled: bool) {
    let ink = if enabled {
        Color32::from_gray(100)
    } else {
        Color32::from_gray(200)
    };
    let r = Rect::from_center_size(
        rect.center(),
        Vec2::new(rect.width() * 0.62, rect.height() * 0.8),
    );
    let fold = r.width() * 0.32;
    let stroke = Stroke::new((rect.width() / 18.0).max(1.0), ink);
    let points = vec![
        r.left_top(),
        Pos2::new(r.right() - fold, r.top()),
        Pos2::new(r.right(), r.top() + fold),
        r.right_bottom(),
        r.left_bottom(),
    ];
    ui.painter()
        .add(egui::Shape::convex_polygon(points, Color32::WHITE, stroke));
    ui.painter().line_segment(
        [
            Pos2::new(r.right() - fold, r.top()),
            Pos2::new(r.right() - fold, r.top() + fold),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            Pos2::new(r.right() - fold, r.top() + fold),
            Pos2::new(r.right(), r.top() + fold),
        ],
        stroke,
    );
}
/// Hangul draws its character-format buttons as a dark "가" in the format itself.
fn draw_glyph(ui: &Ui, rect: Rect, icon: Icon, enabled: bool) -> bool {
    let ink = if enabled {
        Color32::from_gray(45)
    } else {
        Color32::from_gray(190)
    };
    let red = if enabled {
        Color32::from_rgb(230, 30, 30)
    } else {
        Color32::from_gray(200)
    };
    let size = rect.height() * 0.85;
    let mut format = egui::TextFormat {
        font_id: egui::FontId::proportional(size),
        color: ink,
        ..Default::default()
    };
    match icon {
        Icon::Bold => {}
        Icon::Italic => format.italics = true,
        Icon::Underline => format.underline = Stroke::new(1.0, ink),
        Icon::Strike => format.strikethrough = Stroke::new(1.0, ink),
        Icon::TextColor => {}
        _ => return false,
    }
    let mut job = egui::text::LayoutJob::default();
    job.append("가", 0.0, format);
    let galley = ui.painter().layout_job(job);
    let at = rect.center()
        - galley.size() / 2.0
        - egui::vec2(
            0.0,
            if matches!(icon, Icon::TextColor) {
                2.0
            } else {
                0.0
            },
        );
    if matches!(icon, Icon::Bold) {
        // Faux bold: the UI font has a single weight.
        ui.painter()
            .galley(at + egui::vec2(0.6, 0.0), galley.clone(), ink);
    }
    ui.painter().galley(at, galley, ink);
    if matches!(icon, Icon::TextColor) {
        let bar = Rect::from_min_size(
            Pos2::new(
                rect.left() + rect.width() * 0.15,
                rect.bottom() - rect.height() * 0.2,
            ),
            Vec2::new(rect.width() * 0.7, rect.height() * 0.14),
        );
        ui.painter().rect_filled(bar, 0.0, red);
    }
    true
}

/// Shape gallery and lock symbols, drawn as vectors in a grey/blue palette.
fn draw_local(ui: &Ui, rect: Rect, icon: Icon, enabled: bool) {
    let ink = if enabled {
        Color32::from_gray(90)
    } else {
        Color32::from_gray(200)
    };
    let blue = if enabled {
        Color32::from_rgb(48, 110, 230)
    } else {
        Color32::from_gray(205)
    };
    let p = ui.painter();
    let r = rect.shrink(rect.width() * 0.18);
    let stroke = Stroke::new((rect.width() / 16.0).max(1.0), ink);
    match icon {
        Icon::Lock => {
            let body = Rect::from_min_max(
                Pos2::new(r.left(), r.center().y - r.height() * 0.05),
                r.right_bottom(),
            );
            p.rect_filled(body, 1.5, blue);
            let shackle = Rect::from_min_max(
                Pos2::new(r.left() + r.width() * 0.22, r.top()),
                Pos2::new(r.right() - r.width() * 0.22, body.top() + 1.0),
            );
            p.rect_stroke(
                shackle,
                r.width() * 0.25,
                Stroke::new(stroke.width * 1.3, ink),
                egui::StrokeKind::Middle,
            );
        }
        Icon::Rectangle => {
            p.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Middle);
        }
        Icon::Ellipse => {
            p.add(egui::Shape::ellipse_stroke(
                r.center(),
                r.size() / 2.0,
                stroke,
            ));
        }
        Icon::Line => {
            p.line_segment([r.left_bottom(), r.right_top()], stroke);
        }
        Icon::Arc => {
            let points: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.0 + i as f32 / 24.0);
                    Pos2::new(
                        r.right() + r.width() * a.cos(),
                        r.bottom() + r.height() * a.sin(),
                    )
                })
                .collect();
            p.add(egui::Shape::line(points, stroke));
        }
        Icon::Polygon => {
            let points = vec![
                Pos2::new(r.left(), r.center().y),
                Pos2::new(r.left() + r.width() * 0.35, r.top()),
                Pos2::new(r.right(), r.top() + r.height() * 0.3),
                Pos2::new(r.right() - r.width() * 0.2, r.bottom()),
                Pos2::new(r.left() + r.width() * 0.2, r.bottom()),
            ];
            p.add(egui::Shape::closed_line(points, stroke));
        }
        Icon::Curve | Icon::Freeline => {
            let wave = if matches!(icon, Icon::Curve) {
                1.0
            } else {
                2.0
            };
            let points: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let t = i as f32 / 16.0;
                    Pos2::new(
                        r.left() + r.width() * t,
                        r.center().y - (t * std::f32::consts::TAU * wave).sin() * r.height() * 0.35,
                    )
                })
                .collect();
            p.add(egui::Shape::line(points, stroke));
        }
        Icon::VTextbox => {
            p.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Middle);
            for f in [0.35, 0.65] {
                let x = r.left() + r.width() * f;
                p.line_segment(
                    [Pos2::new(x, r.top() + 3.0), Pos2::new(x, r.bottom() - 3.0)],
                    Stroke::new(stroke.width, blue),
                );
            }
        }
        Icon::Connector => {
            let a = r.left_bottom();
            let b = r.right_top();
            p.line_segment([a, b], stroke);
            p.circle_filled(a, stroke.width * 1.6, blue);
            p.circle_filled(b, stroke.width * 1.6, blue);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atlas_holds_every_tile() {
        let image = image::load_from_memory(include_bytes!("../../../assets/toolbar-icons.png"))
            .expect("atlas");
        let capacity =
            (image.width() as usize / TILE as usize) * (image.height() as usize / TILE as usize);
        assert_eq!(image.width() as usize, COLUMNS * TILE as usize);
        assert!((TILES..TILES + COLUMNS).contains(&capacity));
    }
}
