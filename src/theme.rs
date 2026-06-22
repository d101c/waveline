//! Palette de couleurs centralisée. Un seul endroit à toucher pour reskinner.

use ratatui::style::Color;

pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub dim: Color,
    pub accent: Color,
    pub soundcloud: Color,
    pub mixcloud: Color,
    pub selection_bg: Color,
    pub border: Color,
    pub border_active: Color,
    pub playing: Color,
}

impl Theme {
    /// Thème sombre par défaut, lisible sur fond de terminal noir.
    pub fn dark() -> Self {
        Theme {
            bg: Color::Reset,
            fg: Color::Rgb(0xE6, 0xE6, 0xE6),
            dim: Color::Rgb(0x88, 0x88, 0x88),
            accent: Color::Rgb(0x6C, 0xB6, 0xFF),
            soundcloud: Color::Rgb(0xFF, 0x55, 0x00),
            mixcloud: Color::Rgb(0x52, 0xA8, 0xC8),
            selection_bg: Color::Rgb(0x2A, 0x3A, 0x52),
            border: Color::Rgb(0x44, 0x44, 0x44),
            border_active: Color::Rgb(0x6C, 0xB6, 0xFF),
            playing: Color::Rgb(0x5C, 0xE0, 0x8A),
        }
    }

    /// Couleur associée à une plateforme (pour la pastille SC/MC).
    pub fn platform(&self, p: crate::model::Platform) -> Color {
        match p {
            crate::model::Platform::SoundCloud => self.soundcloud,
            crate::model::Platform::Mixcloud => self.mixcloud,
        }
    }
}
