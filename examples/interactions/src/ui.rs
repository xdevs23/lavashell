// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    copy::{CopyKey, copy},
    model::Demo,
};
use lavashell::prelude::*;
use lavashell_example_support::{BODY, Breakpoint, DISPLAY, MONO};

pub const BG: Color = Color::rgb(0x00f4_f1e9);
pub const CARD: Color = Color::rgb(0x00ff_fdf8);
pub const INK: Color = Color::rgb(0x0026_3b42);
pub const MUTED: Color = Color::rgb(0x0062_737a);
pub const LINE: Color = Color::rgb(0x00d8_ddd7);
pub const ACCENT: Color = Color::rgb(0x00c0_5a38);

pub fn label(ui: &mut Ui, key: CopyKey, size: Sp, color: Color) {
    ui.text(
        copy(key),
        Modifier::new()
            .font_family(BODY)
            .font_size(size)
            .color(color)
            .max_lines(4),
    );
}

pub fn data(ui: &mut Ui, value: impl Into<String>, size: Sp, color: Color) {
    ui.text(
        value,
        Modifier::new()
            .font_family(MONO)
            .font_size(size)
            .color(color)
            .max_lines(3),
    );
}

pub fn button(ui: &mut Ui, key: CopyKey, modifier: Modifier, action: impl FnMut() + 'static) {
    let own = Modifier::new()
        .tag(key.to_string())
        .accessible_name(copy(key))
        .border(LINE, 1.dp(), 9.dp())
        .padding_xy(13.dp(), 11.dp());
    ui.button(own.then(modifier), action, |ui| {
        label(ui, key, 13.sp(), INK);
    });
}

pub fn section(ui: &mut Ui, index: &str, key: CopyKey, content: impl FnOnce(&mut Ui)) {
    ui.column(
        Modifier::new()
            .gap(15.dp())
            .fill_max_width()
            .background(CARD)
            .border(LINE, 1.dp(), 18.dp())
            .padding(24.dp()),
        |ui| {
            ui.row(
                Modifier::new().gap(12.dp()).align_items(Align::Center),
                |ui| {
                    data(ui, index, 12.sp(), ACCENT);
                    ui.text(
                        copy(key),
                        Modifier::new()
                            .font_family(DISPLAY)
                            .font_size(21.sp())
                            .font_weight(constant!(FontWeight::try_new(650.0)))
                            .color(INK)
                            .semantics_with(|s| s.role = Some(Role::Heading)),
                    );
                },
            );
            content(ui);
        },
    );
}

pub fn compose(ui: &mut Ui, demo: &Demo, clipboard: &Clipboard, actions: &ViewActions) {
    ui.provide(
        &INTERACTION_STYLE,
        InteractionStyle {
            states: Some(demo.styles.clone()),
            caret: Some(ACCENT),
            selection: Some(ACCENT.with_alpha(0.20)),
            ..Default::default()
        },
        |ui| {
            ui.scroll_column(
                Modifier::new()
                    .focusable(false)
                    .gap(24.dp())
                    .fill_max_width()
                    .fill_max_height()
                    .background(BG)
                    .padding(32.dp()),
                |ui| {
                    ui.row(Modifier::new().align_items(Align::Center), |ui| {
                        ui.column(Modifier::new().gap(9.dp()), |ui| {
                            ui.text(
                                copy(CopyKey::Title),
                                Modifier::new()
                                    .font_family(DISPLAY)
                                    .font_size(32.sp())
                                    .font_weight(constant!(FontWeight::try_new(650.0)))
                                    .color(INK)
                                    .semantics_with(|s| s.role = Some(Role::Heading)),
                            );
                            label(ui, CopyKey::Intro, 15.sp(), MUTED);
                        });
                        ui.space(Modifier::new());
                        data(ui, "01 / 04", 12.sp(), MUTED);
                    });

                    sections(ui, demo, clipboard, actions);
                },
            );
        },
    );
}

/// Composes the four sections in two rows of two columns when the smallest widths of the columns fit the window.
/// Otherwise the sections are composed one below the other.
fn sections(ui: &mut Ui, demo: &Demo, clipboard: &Clipboard, actions: &ViewActions) {
    let columns = Breakpoint::new(ui, "sections");
    let side_by_side = columns.fits(|full, given| full.minimum.width <= given.width);
    let container = Modifier::new().gap(24.dp()).fill_max_width();
    ui.column(container.then(columns.container(side_by_side)), |ui| {
        if side_by_side {
            ui.row(Modifier::new().gap(24.dp()), |ui| {
                ui.column(Modifier::new().weight(1.0), |ui| {
                    crate::text::compose(ui, demo, clipboard, actions);
                });
                ui.column(Modifier::new().weight(1.0), |ui| {
                    crate::transfers::compose(ui, demo, clipboard);
                });
            });
            ui.row(Modifier::new().gap(24.dp()), |ui| {
                ui.column(Modifier::new().weight(1.0), |ui| {
                    crate::states::compose(ui, demo);
                });
                ui.column(Modifier::new().weight(1.0), |ui| {
                    crate::shape::compose(ui, demo, actions);
                });
            });
        } else {
            crate::text::compose(ui, demo, clipboard, actions);
            crate::states::compose(ui, demo);
            crate::transfers::compose(ui, demo, clipboard);
            crate::shape::compose(ui, demo, actions);
        }
    });
}
