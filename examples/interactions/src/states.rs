// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    copy::{CopyKey, copy},
    model::Demo,
    ui,
};
use lavashell::prelude::*;
use std::time::Duration;

pub fn styles() -> StateStyles {
    StateStyles::new(|state| {
        let tint = if state.disabled {
            ui::MUTED.with_alpha(0.05)
        } else if state.pointer.active {
            ui::ACCENT.with_alpha(0.23)
        } else if state.pointer.hovered {
            ui::ACCENT.with_alpha(0.10)
        } else {
            Color::TRANSPARENT
        };
        InteractionAppearance::default()
            .tint(tint, 10.dp())
            .outline(BorderStyle {
                color: if state.focus.visible {
                    ui::ACCENT
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
    ui::section(ui, "02", CopyKey::States, |ui| {
        ui::label(ui, CopyKey::StateHint, 14.sp(), ui::MUTED);
        let presses = demo.activations.clone();
        ui.button(
            Modifier::new()
                .tag("state_button")
                .accessible_name(copy(CopyKey::StateButton))
                .state_styles(demo.styles.clone())
                .fill_max_width()
                .border(ui::LINE, 1.dp(), 10.dp())
                .padding(24.dp()),
            move || presses.update(|n| *n += 1),
            |ui| {
                ui.row(
                    Modifier::new().align_items(Align::Center).fill_max_width(),
                    |ui| {
                        ui::label(ui, CopyKey::StateButton, 17.sp(), ui::INK);
                        ui.space(Modifier::new());
                        ui::data(
                            ui,
                            format!("{:03}", demo.activations.get()),
                            17.sp(),
                            ui::ACCENT,
                        );
                    },
                );
            },
        );

        let state = demo.state.get();
        ui.row(Modifier::new().gap(12.dp()).tag("state_readout"), |ui| {
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
                                if active { ui::ACCENT } else { ui::LINE },
                            );
                        });
                        ui::label(ui, key, 12.sp(), if active { ui::INK } else { ui::MUTED });
                    },
                );
            }
        });
        ui::label(ui, CopyKey::KeyboardHint, 13.sp(), ui::MUTED);
    });
}
