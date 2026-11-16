// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    copy::{CopyKey, copy},
    model::Demo,
    palette::{PALETTE, Palette},
    tag::Tag,
    ui,
};
use lavashell::prelude::*;
use std::time::Duration;

pub fn styles(colors: &Palette) -> StateStyles {
    let colors = *colors;
    StateStyles::new(move |state| {
        let tint = if state.disabled {
            colors.muted.with_alpha(0.05)
        } else if state.pointer.active {
            colors.accent.with_alpha(0.23)
        } else if state.pointer.hovered {
            colors.accent.with_alpha(0.10)
        } else {
            Color::TRANSPARENT
        };
        InteractionAppearance::default()
            .tint(tint, 10.dp())
            .outline(BorderStyle {
                color: if state.focus.visible {
                    colors.accent
                } else {
                    Color::TRANSPARENT
                },
                width: if state.focus.visible {
                    constant!(NonNegativeDp::try_new(Dp::new(2.0)))
                } else {
                    NonNegativeDp::ZERO
                },
                radius: 10.dp(),
            })
    })
    .motion(Tween::new(Duration::from_millis(160)))
}

pub fn compose(ui: &mut Ui, demo: &Demo) {
    let colors = ui.local(&PALETTE);
    ui::section(ui, "02", CopyKey::States, |ui| {
        let presses = demo.activations.clone();
        ui.button(
            Modifier::new()
                .tag(Tag::Control(CopyKey::StateButton))
                .accessible_name(copy(CopyKey::StateButton))
                .state_styles(styles(&colors))
                .fill_max_width()
                .border(colors.line, 1.dp(), 10.dp())
                .padding(24.dp()),
            move || presses.update(|n| *n += 1),
            |ui| {
                ui.row(
                    Modifier::new().align_items(Align::Center).fill_max_width(),
                    |ui| {
                        ui::label(ui, CopyKey::StateButton, 17.sp(), colors.ink);
                        ui.space(Modifier::new());
                        ui::data(
                            ui,
                            format!("{:03}", demo.activations.get()),
                            17.sp(),
                            colors.accent,
                        );
                    },
                );
            },
        );

        let state = demo.state.get();
        ui.row(Modifier::new().gap(12.dp()).tag(Tag::StateReadout), |ui| {
            for (key, active) in [
                (
                    CopyKey::Idle,
                    !state.pointer.hovered && !state.focus.focused && !state.pointer.active,
                ),
                (CopyKey::Hover, state.pointer.hovered),
                (CopyKey::Focus, state.focus.focused),
                (CopyKey::Pressed, state.pointer.active),
            ] {
                ui.row(
                    Modifier::new()
                        .gap(7.dp())
                        .align_items(Align::Center)
                        .weight(1.0),
                    |ui| {
                        ui.canvas(Size::new(7.dp(), 7.dp()), Modifier::new(), move |d| {
                            d.circle(
                                Point::new(3.5.dp(), 3.5.dp()),
                                3.5.dp(),
                                if active { colors.accent } else { colors.line },
                            );
                        });
                        ui::label(
                            ui,
                            key,
                            12.sp(),
                            if active { colors.ink } else { colors.muted },
                        );
                    },
                );
            }
        });
    });
}
