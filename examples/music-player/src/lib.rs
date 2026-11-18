// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod error;
pub mod music;
mod surfaces;
mod theme;

pub use error::Error;
pub use surfaces::Surfaces;

use lavashell::prelude::*;
use lavashell_example_support::{ExampleArguments, Theme, size_window};

pub const APP_ID: &str = "lavashell.music";

pub fn build_app(
    example: &ExampleArguments,
    size: Option<Size>,
    surfaces: Surfaces,
) -> Result<App, Error> {
    let model = State::new(music::Model::default());
    let theme = Theme::new(example.theme())?;

    let mut app = example.app()?;
    if surfaces.has_window() {
        app = self::app(app, &model, &theme, size)?;
    }
    let tick = model.clone();
    app = app.on_tick(move |elapsed| {
        if tick.with(|m| m.playing) {
            tick.update(|m| m.advance(elapsed));
        }
    });
    if surfaces.has_layer() {
        let (state, icons) = (model.clone(), music::Icons::decode()?);
        app.layer(LayerSurface::new("lavashell-music"), move |ui| {
            music::panel(ui, &state, &theme, icons);
        });
    }
    Ok(app)
}

pub fn build_window_app(example: &ExampleArguments, size: Option<Size>) -> Result<App, Error> {
    let model = State::new(music::Model::default());
    app(example.app()?, &model, &Theme::new(example.theme())?, size)
}

pub fn app(
    mut app: App,
    model: &State<music::Model>,
    theme: &Theme,
    size: Option<Size>,
) -> Result<App, Error> {
    let icons = music::Icons::decode()?;
    let window = size_window(
        Window::new(music::copy(music::CopyKey::Title), APP_ID),
        size,
    );
    let (shown, theme) = (model.clone(), theme.clone());
    app.window(window, move |ui| music::music(ui, &shown, &theme, icons))?;
    Ok(app)
}
