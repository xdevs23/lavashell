// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::*;
use lavashell_example_support::{BODY, DISPLAY};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: Color,
    pub surface: Color,
    pub ink: Color,
    pub muted: Color,
    pub blue: Color,
    pub on_blue: Color,
    pub pale: Color,
    pub line: Color,
    pub amber: Color,
}

impl Palette {
    const LIGHT: Self = Self {
        background: Color::rgb(0x00e9_eef5),
        surface: Color::rgb(0x00ff_ffff),
        ink: Color::rgb(0x0024_364b),
        muted: Color::rgb(0x0068_7d95),
        blue: Color::rgb(0x0031_5ddd),
        on_blue: Color::WHITE,
        pale: Color::rgb(0x00dc_e7fb),
        line: Color::rgb(0x00d8_e0eb),
        amber: Color::rgb(0x00ef_ad49),
    };

    const DARK: Self = Self {
        background: Color::rgb(0x000f_1722),
        surface: Color::rgb(0x0018_2433),
        ink: Color::rgb(0x00e6_edf6),
        muted: Color::rgb(0x0093_a7bf),
        blue: Color::rgb(0x007b_97ff),
        on_blue: Color::rgb(0x000b_1226),
        pale: Color::rgb(0x0024_344f),
        line: Color::rgb(0x002c_3d57),
        amber: Color::rgb(0x00ef_ad49),
    };

    pub fn of(scheme: ColorScheme) -> Self {
        scheme.select(Self::DARK, Self::LIGHT)
    }
}

pub static PALETTE: Local<Palette> = Local::platform(|platform| Palette::of(platform.color_scheme));

pub fn body(ink: Color) -> TextStyle {
    TextStyle::new(BODY, 14.sp(), ink)
}

pub fn title(ink: Color, size: Sp) -> TextStyle {
    TextStyle::new(DISPLAY, size, ink).weight(FontWeight::BOLD)
}

pub fn interaction(colors: &Palette) -> InteractionStyle {
    InteractionStyle {
        focus: Some(BorderStyle {
            color: colors.blue,
            width: constant!(NonNegativeDp::try_new(Dp::new(2.0))),
            radius: 8.dp(),
        }),
        ..Default::default()
    }
}

pub fn button(colors: &Palette) -> Modifier {
    Modifier::new()
        .clip(8.dp())
        .background(colors.pale)
        .padding_xy(15.dp(), 10.dp())
}
