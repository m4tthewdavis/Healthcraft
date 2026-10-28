//! Design tokens. Colours come from here, never from literals in view code, so a theme (and a
//! high-contrast mode) changes one place.

use egui::Color32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    pub accent: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub muted: Color32,
}

impl Default for Tokens {
    fn default() -> Self {
        Self {
            accent: Color32::from_rgb(0x2f, 0x7b, 0xf5),
            warning: Color32::from_rgb(0xd6, 0x9e, 0x2e),
            danger: Color32::from_rgb(0xe0, 0x4f, 0x4f),
            muted: Color32::from_rgb(0x8a, 0x8f, 0x98),
        }
    }
}
