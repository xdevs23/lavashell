// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: Color,
    pub card: Color,
    pub ink: Color,
    pub muted: Color,
    pub line: Color,
    pub accent: Color,
}

impl Palette {
    const LIGHT: Self = Self {
        background: Color::rgb(0x00f4_f1e9),
        card: Color::rgb(0x00ff_fdf8),
        ink: Color::rgb(0x0026_3b42),
        muted: Color::rgb(0x0062_737a),
        line: Color::rgb(0x00d8_ddd7),
        accent: Color::rgb(0x00c0_5a38),
    };

    const DARK: Self = Self {
        background: Color::rgb(0x0019_1b1c),
        card: Color::rgb(0x0023_2625),
        ink: Color::rgb(0x00ec_e8df),
        muted: Color::rgb(0x00a3_aaa8),
        line: Color::rgb(0x0038_3d3b),
        accent: Color::rgb(0x00e8_8a66),
    };

    pub fn of(scheme: ColorScheme) -> Self {
        scheme.select(Self::DARK, Self::LIGHT)
    }
}

pub static PALETTE: Local<Palette> = Local::platform(|platform| Palette::of(platform.color_scheme));
