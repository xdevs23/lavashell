// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::*;
use lavashell_example_support::{BODY, DISPLAY};

pub const BG: Color = Color::rgb(0x0010_2833);
pub const SIDE: Color = Color::rgb(0x000b_1d28);
pub const CARD: Color = Color::rgb(0x0017_3640);
pub const ACTIVE: Color = Color::rgb(0x0021_454e);
pub const LINE: Color = Color::rgb(0x0028_4650);
pub const TEXT: Color = Color::rgb(0x00f0_f4ed);
pub const MUTED: Color = Color::rgb(0x0091_abb1);
pub const ACCENT: Color = Color::rgb(0x008d_e0d1);

pub fn body() -> TextStyle {
    TextStyle::new(BODY, 14.sp(), TEXT)
}

pub fn title(size: Sp) -> TextStyle {
    TextStyle::new(DISPLAY, size, TEXT).weight(FontWeight::BOLD)
}

pub fn interaction() -> InteractionStyle {
    InteractionStyle {
        focus: Some(BorderStyle {
            color: ACCENT,
            width: constant!(NonNegativeDp::try_new(Dp::new(2.0))),
            radius: 5.dp(),
        }),
        placeholder: Some(MUTED),
        caret: Some(ACCENT),
        selection: Some(ACTIVE),
        tooltip: Some(TooltipStyle {
            text: body().size(12.sp()),
            background: SIDE,
            border: BorderStyle {
                color: LINE,
                width: constant!(NonNegativeDp::try_new(Dp::new(1.0))),
                radius: 6.dp(),
            },
            padding: Insets::xy(10.dp(), 7.dp()),
            gap: 8.dp(),
        }),
        ..Default::default()
    }
}

pub fn scope(ui: &mut Ui, children: impl FnOnce(&mut Ui)) {
    ui.provide(&TEXT_STYLE, body(), |ui| {
        ui.provide(&INTERACTION_STYLE, interaction(), children);
    });
}
