// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::prelude::*;
use lavashell_example_support::{
    Error, TEAL_PALETTE, TealPalette, Theme, ThemeChoice, ThemeSwitchColors, exit_code, icon,
    platform,
};
use std::process::ExitCode;

fn counter(ui: &mut Ui, theme: &Theme, plus: &'static Icon) {
    theme.provide(ui, &TEAL_PALETTE, TealPalette::of, |ui| {
        let colors = ui.local(&TEAL_PALETTE);
        ui.key("counter", Modifier::new(), |ui| {
            let count = ui.state("count", || 0_u32);
            ui.column(
                Modifier::new()
                    .gap(24.dp())
                    .align_items(Align::Center)
                    .fill_max_width()
                    .fill_max_height()
                    .background(colors.background)
                    .padding(32.dp()),
                |ui| {
                    ui.row(Modifier::new().fill_max_width(), |ui| {
                        ui.space(Modifier::new());
                        theme.switch(
                            ui,
                            ThemeSwitchColors {
                                ink: colors.muted,
                                accent: colors.accent,
                            },
                        );
                    });
                    ui.text(
                        count.get().to_string(),
                        Modifier::new().font_size(48.sp()).color(colors.text),
                    );
                    ui.icon_button(
                        plus,
                        IconOptions {
                            size: Some(24.dp()),
                            ..IconOptions::default()
                        },
                        Modifier::new()
                            .color(colors.on_accent)
                            .clip(24.dp())
                            .background(colors.accent)
                            .padding(12.dp()),
                        count.action(|n| *n += 1),
                    );
                },
            );
        });
    });
}

fn main() -> ExitCode {
    exit_code(run())
}

fn run() -> Result<(), Error> {
    let plus = icon(&lavashell_lucide::PLUS)?;
    let theme = Theme::new(ThemeChoice::System)?;
    let window = Window::new("Lavashell / Counter", "lavashell.counter");
    let mut app = App::new(platform());
    app.window(window, move |ui| counter(ui, &theme, plus))?;
    lavashell::run::<Platform>(app, lavashell::setup()).map_err(Error::App)
}
