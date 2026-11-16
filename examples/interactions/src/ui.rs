// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    copy::{CopyKey, copy},
    model::Demo,
    palette::{PALETTE, Palette},
    states,
    tag::Tag,
};
use lavashell::prelude::*;
use lavashell_example_support::{BODY, Breakpoint, DISPLAY, Layout, MONO, ThemeSwitchColors};

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
    let colors = ui.local(&PALETTE);
    let own = Modifier::new()
        .tag(Tag::Control(key))
        .accessible_name(copy(key))
        .border(colors.line, 1.dp(), 9.dp())
        .padding_xy(13.dp(), 11.dp());
    ui.button(own.then(modifier), action, |ui| {
        label(ui, key, 13.sp(), colors.ink);
    });
}

pub fn section(ui: &mut Ui, index: &str, key: CopyKey, content: impl FnOnce(&mut Ui)) {
    let colors = ui.local(&PALETTE);
    ui.column(
        Modifier::new()
            .gap(15.dp())
            .fill_max_width()
            .background(colors.card)
            .border(colors.line, 1.dp(), 18.dp())
            .padding(24.dp()),
        |ui| {
            ui.row(
                Modifier::new().gap(12.dp()).align_items(Align::Center),
                |ui| {
                    data(ui, index, 12.sp(), colors.accent);
                    ui.text(
                        copy(key),
                        Modifier::new()
                            .font_family(DISPLAY)
                            .font_size(21.sp())
                            .font_weight(constant!(FontWeight::try_new(650.0)))
                            .color(colors.ink)
                            .semantics_with(|s| s.role = Some(Role::Heading)),
                    );
                },
            );
            content(ui);
        },
    );
}

pub fn themed(ui: &mut Ui, demo: &Demo, children: impl FnOnce(&mut Ui)) {
    demo.theme.provide(ui, &PALETTE, Palette::of, |ui| {
        let colors = ui.local(&PALETTE);
        ui.provide(
            &INTERACTION_STYLE,
            InteractionStyle {
                states: Some(states::styles(&colors)),
                caret: Some(colors.accent),
                selection: Some(colors.accent.with_alpha(0.20)),
                ..Default::default()
            },
            children,
        );
    });
}

pub fn theme_switch(ui: &mut Ui, demo: &Demo) {
    let colors = ui.local(&PALETTE);
    demo.theme.switch(
        ui,
        ThemeSwitchColors {
            ink: colors.muted,
            accent: colors.accent,
        },
    );
}

pub fn compose(ui: &mut Ui, demo: &Demo, clipboard: &Clipboard, actions: &ViewActions) {
    themed(ui, demo, |ui| {
        let colors = ui.local(&PALETTE);
        ui.scroll_column(
            Modifier::new()
                .focusable(false)
                .gap(24.dp())
                .fill_max_width()
                .fill_max_height()
                .background(colors.background)
                .padding(32.dp()),
            |ui| {
                ui.row(Modifier::new().align_items(Align::Center), |ui| {
                    ui.text(
                        copy(CopyKey::Title),
                        Modifier::new()
                            .font_family(DISPLAY)
                            .font_size(32.sp())
                            .font_weight(constant!(FontWeight::try_new(650.0)))
                            .color(colors.ink)
                            .semantics_with(|s| s.role = Some(Role::Heading)),
                    );
                    ui.space(Modifier::new());
                    theme_switch(ui, demo);
                    data(ui, "01 / 04", 12.sp(), colors.muted);
                });

                sections(ui, demo, clipboard, actions);
            },
        );
    });
}

fn sections(ui: &mut Ui, demo: &Demo, clipboard: &Clipboard, actions: &ViewActions) {
    let columns = Breakpoint::new(ui, "sections");
    let layout = columns.layout(|full, given| full.minimum.width <= given.width);
    let container = Modifier::new().gap(24.dp()).fill_max_width();
    ui.column(container.then(layout.container(&columns)), |ui| {
        if layout == Layout::Regular {
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
