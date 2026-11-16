// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TealPalette {
    pub background: Color,
    pub sidebar: Color,
    pub card: Color,
    pub active: Color,
    pub line: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub on_accent: Color,
}

impl TealPalette {
    pub const DARK: Self = Self {
        background: Color::rgb(0x0010_2833),
        sidebar: Color::rgb(0x000b_1d28),
        card: Color::rgb(0x0017_3640),
        active: Color::rgb(0x0021_454e),
        line: Color::rgb(0x0028_4650),
        text: Color::rgb(0x00f0_f4ed),
        muted: Color::rgb(0x0091_abb1),
        accent: Color::rgb(0x008d_e0d1),
        on_accent: Color::rgb(0x000b_1d28),
    };

    pub const LIGHT: Self = Self {
        background: Color::rgb(0x00ee_f5f3),
        sidebar: Color::rgb(0x00dc_e9e6),
        card: Color::rgb(0x00ff_ffff),
        active: Color::rgb(0x00cb_e5e0),
        line: Color::rgb(0x00c2_d6d2),
        text: Color::rgb(0x0012_303a),
        muted: Color::rgb(0x0040_606a),
        accent: Color::rgb(0x000b_645d),
        on_accent: Color::rgb(0x00f2_fbf9),
    };

    #[must_use]
    pub fn of(scheme: ColorScheme) -> Self {
        scheme.select(Self::DARK, Self::LIGHT)
    }
}

pub static TEAL_PALETTE: Local<TealPalette> =
    Local::platform(|platform| TealPalette::of(platform.color_scheme));

#[cfg(test)]
mod tests;
