//! Shared chrome, adapted from DesignCraft theme tokens and DeckCraft dialog
//! button sizing (MIT). Palette and arrangement follow observed HWP windows.
use egui::{Color32, Frame, Id, Modal, RichText, Stroke, Ui, Vec2};
pub struct Tokens;
impl Tokens {
    pub const ACCENT: Color32 = Color32::from_rgb(35, 103, 255);
    pub const TITLE: Color32 = Color32::from_rgb(233, 240, 255);
    pub const BORDER: Color32 = Color32::from_rgb(200, 200, 200);
    /// Dialog window outline (Hangul-style dialogs).
    pub const FRAME: Color32 = Color32::from_rgb(123, 164, 255);
    pub const PANEL: Color32 = Color32::WHITE;
    pub const INPUT: Color32 = Color32::from_rgb(252, 252, 252);
}
pub fn modal(id: impl std::hash::Hash + std::fmt::Debug) -> Dialog {
    Dialog(
        Modal::new(Id::new(id))
            .backdrop_color(Color32::from_black_alpha(8))
            .frame(
                Frame::new()
                    .fill(Tokens::PANEL)
                    .stroke(Stroke::new(1.0, Tokens::FRAME))
                    .corner_radius(0)
                    .inner_margin(12),
            ),
    )
}
/// A modal dialog window. One taller than the app window (800×600, or a tall
/// preview) scrolls inside it instead of running off screen with its title and
/// buttons out of reach; [`crate::ext_shell::dialog`] also narrows to the window.
pub struct Dialog(Modal);
impl Dialog {
    pub fn show<R>(
        self,
        ctx: &egui::Context,
        body: impl FnOnce(&mut Ui) -> R,
    ) -> egui::ModalResponse<R> {
        // The frame's margin and stroke take 26 px; keep 8 px clear of the window edge.
        let room = ctx.content_rect().size() - Vec2::splat(42.0);
        self.0.show(ctx, |ui| {
            // Hangul leaves a wider gap than egui's 4 px between options on one row.
            ui.spacing_mut().item_spacing.x = 8.0;
            // The modal area does not know its height on the first frame; the minimum lets
            // the viewport grow to the window instead of the area's first guess.
            egui::ScrollArea::vertical()
                .max_height(room.y.max(150.0))
                .min_scrolled_height(room.y.max(150.0))
                .show(ui, body)
                .inner
        })
    }
}
/// A colour button whose outline shows on a dialog: egui draws the 1 px outline in the
/// widget fill, which [`title`] makes white, so a white colour vanished into the page.
pub fn color_button(ui: &mut Ui, color: &mut Color32) -> egui::Response {
    ui.scope(|ui| {
        ui.visuals_mut().widgets.inactive.bg_fill = Color32::from_gray(150);
        ui.color_edit_button_srgba(color)
    })
    .inner
}
/// [`color_button`] for an RGB value (no alpha control: the value is saved without one).
pub fn color_button_rgb(ui: &mut Ui, rgb: &mut [u8; 3]) -> egui::Response {
    ui.scope(|ui| {
        ui.visuals_mut().widgets.inactive.bg_fill = Color32::from_gray(150);
        egui::color_picker::color_edit_button_srgb(ui, rgb)
    })
    .inner
}
pub fn title(ui: &mut Ui, label: &str) -> bool {
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
    visuals.widgets.inactive.bg_fill = Color32::WHITE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Tokens::BORDER);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(2);
    visuals.extreme_bg_color = Color32::WHITE;
    let mut close = false;
    Frame::new()
        .fill(Tokens::TITLE)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = ui
                        .add(egui::Button::new("×").frame(false))
                        .on_hover_text("닫기")
                        .clicked();
                });
            });
        });
    ui.add_space(10.0);
    close
}
pub fn section(ui: &mut Ui, label: &str) {
    ui.add_space(7.0);
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::Separator::default().horizontal());
    });
    ui.add_space(3.0);
}
pub fn tabs(ui: &mut Ui, selected: &mut usize, labels: &[&str]) {
    ui.horizontal(|ui| {
        for (i, label) in labels.iter().enumerate() {
            // Frameless tabs need more room than the space inside a name, or "문단 번호"
            // and "그림 글머리표" read as one tab (a frameless button has no padding).
            if i > 0 {
                ui.add_space(14.0);
            }
            let r = ui.add(
                egui::Button::new(*label)
                    .frame(false)
                    .min_size(Vec2::new(56.0, 28.0)),
            );
            if r.clicked() {
                *selected = i;
            }
            if *selected == i {
                ui.painter().line_segment(
                    [r.rect.left_bottom(), r.rect.right_bottom()],
                    Stroke::new(2.0, Tokens::ACCENT),
                );
            }
        }
    });
    ui.separator();
}
/// Give a dialog's first text field the keyboard when the dialog opens: while nothing,
/// or only the document page behind the dialog, holds it.
pub fn first_focus(field: &egui::Response) {
    let canvas = egui::Id::new("document-canvas");
    if field
        .ctx
        .memory(|m| m.focused().is_none_or(|id| id == canvas))
    {
        field.request_focus();
    }
}
/// The key of a button label's Hangul access letter, "설정(D)" → D.
pub fn mnemonic(label: &str) -> Option<egui::Key> {
    let inner = label.strip_suffix(')')?.rsplit_once('(')?.1;
    let mut chars = inner.chars();
    let c = chars.next().filter(|c| c.is_ascii_alphanumeric())?;
    if chars.next().is_some() {
        return None;
    }
    egui::Key::from_name(&c.to_ascii_uppercase().to_string())
}
/// The apply and 취소 buttons of a dialog. `grey` is why apply is unavailable (shown on
/// the grey button), None when it can run. Enter applies (unless a text field has the
/// keyboard) and Alt + the label's access letter applies too, as in the shell dialogs.
pub fn actions(ui: &mut Ui, grey: Option<&str>, label: &str) -> (bool, bool) {
    ui.set_width(82.0);
    let ctx = ui.ctx().clone();
    let valid = grey.is_none();
    let keyed = valid
        && (mnemonic(label)
            .is_some_and(|key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, key)))
            || (!ctx.egui_wants_keyboard_input()
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))));
    let apply = ui
        .add_enabled(
            valid,
            egui::Button::new(label)
                .fill(Color32::WHITE)
                .stroke(Stroke::new(1.5, Tokens::ACCENT))
                .min_size(Vec2::new(80.0, 28.0)),
        )
        .on_disabled_hover_text(grey.unwrap_or_default())
        .clicked()
        || keyed;
    let cancel = ui
        .add(egui::Button::new("취소").min_size(Vec2::new(80.0, 28.0)))
        .clicked();
    (apply, cancel)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A white colour keeps a visible outline in a dialog (문서 보기 색's 바탕 색).
    #[test]
    fn a_white_colour_button_has_a_visible_outline_in_a_dialog() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            title(ui, "색");
            let mut white = Color32::WHITE;
            color_button(ui, &mut white);
        });
        // Debug builds check that a dropped output frees its font atlas delta.
        out.textures_delta.clear();
        let outlines: Vec<Color32> = out
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Rect(r) if r.stroke.width > 0.0 => Some(r.stroke.color),
                _ => None,
            })
            .collect();
        assert!(
            outlines.iter().any(|c| *c == Color32::from_gray(150)),
            "outlines painted: {outlines:?}"
        );
    }
}
