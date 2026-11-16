// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::*;

const SCALE: TonalPalette = TonalPalette::new(80.0, 0.018);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: Color,
    pub chip: Color,
    pub ink: Color,
    pub muted: Color,
    pub line: Color,
    pub accent: Color,
}

impl Palette {
    const DARK: Self = Self {
        background: SCALE.tone(0.17),
        chip: SCALE.tone(0.23),
        ink: SCALE.tone(0.95),
        muted: SCALE.tone(0.66),
        line: SCALE.tone(0.31),
        accent: Color::oklch(0.84, 0.11, 80.0),
    };

    const LIGHT: Self = Self {
        background: SCALE.tone(0.96),
        chip: SCALE.tone(0.9),
        ink: SCALE.tone(0.2),
        muted: SCALE.tone(0.46),
        line: SCALE.tone(0.8),
        accent: Color::oklch(0.5, 0.12, 80.0),
    };

    pub fn of(scheme: ColorScheme) -> Self {
        scheme.select(Self::DARK, Self::LIGHT)
    }
}

pub static PALETTE: Local<Palette> = Local::platform(|platform| Palette::of(platform.color_scheme));
