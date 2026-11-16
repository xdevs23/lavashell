// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::*;
use lavashell_example_support::{BODY, DISPLAY, TEAL_PALETTE, TealPalette, Theme};

pub fn body(color: Color) -> TextStyle {
    TextStyle::new(BODY, 14.sp(), color)
}

pub fn title(color: Color, size: Sp) -> TextStyle {
    TextStyle::new(DISPLAY, size, color).weight(FontWeight::BOLD)
}

fn interaction(colors: &TealPalette) -> InteractionStyle {
    InteractionStyle {
        focus: Some(BorderStyle {
            color: colors.accent,
            width: constant!(NonNegativeDp::try_new(Dp::new(2.0))),
            radius: 5.dp(),
        }),
        placeholder: Some(colors.muted),
        caret: Some(colors.accent),
        selection: Some(colors.active),
        tooltip: Some(TooltipStyle {
            text: body(colors.text).size(12.sp()),
            background: colors.sidebar,
            border: BorderStyle {
                color: colors.line,
                width: constant!(NonNegativeDp::try_new(Dp::new(1.0))),
                radius: 6.dp(),
            },
            padding: Insets::xy(10.dp(), 7.dp()),
            gap: 8.dp(),
        }),
        ..Default::default()
    }
}

pub fn scope(ui: &mut Ui, theme: &Theme, children: impl FnOnce(&mut Ui)) {
    theme.provide(ui, &TEAL_PALETTE, TealPalette::of, |ui| {
        let colors = ui.local(&TEAL_PALETTE);
        ui.provide(&TEXT_STYLE, body(colors.text), |ui| {
            ui.provide(&INTERACTION_STYLE, interaction(&colors), children);
        });
    });
}
