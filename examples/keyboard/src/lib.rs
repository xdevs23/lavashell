// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod error;
mod look;
pub mod model;
pub mod recording;
pub mod scene;
pub mod stage;

pub use error::Error;

use lavashell::prelude::*;
use model::Demo;

include!(concat!(env!("OUT_DIR"), "/copy.rs"));

/// `app` with the scene of `demo` in a window of `size`, which covers the screen when `fullscreen` is set. The arrow
/// keys and Enter move and press the focus ring of the keyboard while it is shown.
///
/// # Errors
/// Returns an error when the window cannot be added to `app`.
pub fn app(app: App, demo: &Demo, size: Size, fullscreen: bool) -> Result<App, Error> {
    let arrow_keys = demo.clone();
    let mut app = app.input_filter(move |_, input| arrow_keys.route(input));
    let shown = demo.clone();
    app.window(
        Window::new(copy(CopyKey::Title), "lavashell.keyboard")
            .size(size.width, size.height)
            .fullscreen(fullscreen),
        move |ui| scene::compose(ui, &shown),
    )?;
    Ok(app)
}
