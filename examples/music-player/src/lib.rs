// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod error;
pub mod music;
mod theme;

pub use error::Error;

use lavashell::prelude::*;

pub const APP_ID: &str = "lavashell.music";

/// `app` with the library of `model` in a window of `size`, or of the size measured for the library when `size` is
/// absent.
///
/// # Errors
/// Returns an error when the icons cannot be decoded or the window cannot be added to `app`.
pub fn app(mut app: App, model: &State<music::Model>, size: Option<Size>) -> Result<App, Error> {
    let icons = music::Icons::decode()?;
    let window = Window::new(music::copy(music::CopyKey::Title), APP_ID);
    let window = match size {
        Some(size) => window.size(size.width, size.height),
        None => window,
    };
    let shown = model.clone();
    app.window(window, move |ui| music::music(ui, &shown, icons))?;
    Ok(app)
}
