// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod error;
pub mod music;
mod theme;

pub use error::Error;

use lavashell::prelude::*;
use lavashell_example_support::{Theme, size_window};

pub const APP_ID: &str = "lavashell.music";

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
