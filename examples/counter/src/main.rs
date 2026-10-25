// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use lavashell::prelude::*;
use lavashell_example_support::{Error, exit_code, icon, platform};
use std::process::ExitCode;

fn counter(ui: &mut Ui, plus: &'static Icon) {
    ui.key("counter", Modifier::new(), |ui| {
        let count = ui.state("count", || 0_u32);
        ui.column(
            Modifier::new()
                .gap(24.dp())
                .align_items(Align::Center)
                .fill_max_width()
                .fill_max_height()
                .background(Color::rgb(0x0010_2833))
                .padding(32.dp()),
            |ui| {
                ui.text(
                    count.get().to_string(),
                    Modifier::new().font_size(48.sp()).color(Color::WHITE),
                );
                ui.icon_button(
                    plus,
                    IconOptions {
                        size: Some(24.dp()),
                        ..IconOptions::default()
                    },
                    Modifier::new()
                        .color(Color::rgb(0x000b_1d28))
                        .clip(24.dp())
                        .background(Color::rgb(0x008d_e0d1))
                        .padding(12.dp()),
                    count.action(|n| *n += 1),
                );
            },
        );
    });
}

fn main() -> ExitCode {
    exit_code(run())
}

fn run() -> Result<(), Error> {
    let plus = icon(&lavashell_lucide::PLUS)?;
    let window = Window::new("Lavashell / Counter", "lavashell.counter");
    App::new(platform())
        .run(|app| app.window(window, move |ui| counter(ui, plus)))
        .map_err(Error::App)
}
