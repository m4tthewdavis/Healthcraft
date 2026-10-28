//! Design tokens. Colours come from here, never from literals in view code, so a theme (and a
//! high-contrast mode) changes one place.

use egui::{Color32, CornerRadius, Frame, Margin, Stroke, Ui};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    pub accent: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub muted: Color32,
    pub ok: Color32,
}

impl Default for Tokens {
    fn default() -> Self {
        Self {
            accent: Color32::from_rgb(0x2f, 0x7b, 0xf5),
            warning: Color32::from_rgb(0xd6, 0x9e, 0x2e),
            danger: Color32::from_rgb(0xe0, 0x4f, 0x4f),
            muted: Color32::from_rgb(0x8a, 0x8f, 0x98),
            ok: Color32::from_rgb(0x2e, 0xa0, 0x6b),
        }
    }
}

/// Type sizes (points).
pub const TITLE: f32 = 22.0;
pub const SECTION: f32 = 15.0;
pub const BODY: f32 = 14.0;
pub const SMALL: f32 = 12.0;

/// A content card: a faint fill and hairline border, used for one self-contained block.
pub fn card(ui: &Ui) -> Frame {
    let v = ui.visuals();
    Frame::new().fill(v.faint_bg_color).stroke(v.widgets.noninteractive.bg_stroke).corner_radius(CornerRadius::same(8)).inner_margin(Margin::same(12))
}

/// The patient banner: like a card, with an accent rule on its border.
pub fn banner(ui: &Ui, t: &Tokens) -> Frame {
    let v = ui.visuals();
    Frame::new()
        .fill(v.faint_bg_color)
        .stroke(Stroke::new(1.0, t.accent.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(16, 12))
}

/// A small rounded label (status, visit type).
pub fn chip(ui: &mut Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.15))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.5)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| ui.label(egui::RichText::new(text).size(SMALL).color(color)));
}
