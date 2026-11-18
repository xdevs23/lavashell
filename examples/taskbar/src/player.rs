// SPDX-License-Identifier: GPL-3.0-only
use crate::{CopyKey, Error, copy, islands, look, model::Taskbar, paint};
use lavashell::prelude::*;

pub fn app(
    mut app: App,
    taskbar: &Taskbar,
    size: Option<Size>,
    fullscreen: bool,
) -> Result<App, Error> {
    let size = size.unwrap_or(look::SCREEN);
    let shown = taskbar.clone();
    app.window(
        Window::new(copy(CopyKey::Title), "lavashell.taskbar")
            .size(size.width, size.height)
            .fullscreen(fullscreen),
        move |ui| compose(ui, &shown),
    )?;
    Ok(app)
}

fn compose(ui: &mut Ui, taskbar: &Taskbar) {
    let size = ui.viewport().size;
    ui.provide(&TEXT_STYLE, look::body(), |ui| {
        ui.canvas(size, Modifier::new(), paint::map);
        ui.column(
            Modifier::new()
                .fill_max_width()
                .fill_max_height()
                .padding(20.dp()),
            |ui| {
                islands::theme(ui, taskbar);
                ui.space(Modifier::new());
                islands::bar(ui, taskbar);
            },
        );
    });
}
