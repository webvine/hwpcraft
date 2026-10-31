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
pub fn modal(id: impl std::hash::Hash + std::fmt::Debug) -> Modal {
    Modal::new(Id::new(id))
        .backdrop_color(Color32::from_black_alpha(8))
        .frame(
            Frame::new()
                .fill(Tokens::PANEL)
                .stroke(Stroke::new(1.0, Tokens::FRAME))
                .corner_radius(0)
                .inner_margin(12),
        )
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
