// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::*;
use lavashell_example_support::{BODY, DISPLAY};

pub const BACKGROUND: Color = Color::rgb(0x00e9_eef5);
pub const SURFACE: Color = Color::rgb(0x00ff_ffff);
pub const INK: Color = Color::rgb(0x0024_364b);
pub const MUTED: Color = Color::rgb(0x0068_7d95);
pub const BLUE: Color = Color::rgb(0x0031_5ddd);
pub const PALE: Color = Color::rgb(0x00dc_e7fb);
pub const LINE: Color = Color::rgb(0x00d8_e0eb);
pub const AMBER: Color = Color::rgb(0x00ef_ad49);

pub fn body() -> TextStyle {
    TextStyle::new(BODY, 14.sp(), INK)
}

pub fn title(size: Sp) -> TextStyle {
    TextStyle::new(DISPLAY, size, INK).weight(FontWeight::BOLD)
}

pub fn interaction() -> InteractionStyle {
    InteractionStyle {
        focus: Some(BorderStyle {
            color: BLUE,
            width: constant!(NonNegativeDp::try_new(Dp::new(2.0))),
            radius: 8.dp(),
        }),
        ..Default::default()
    }
}

pub fn button() -> Modifier {
    Modifier::new()
        .clip(8.dp())
        .background(PALE)
        .padding_xy(15.dp(), 10.dp())
}
