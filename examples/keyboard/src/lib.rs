// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod error;
mod look;
pub mod model;
pub mod scene;

pub use error::Error;

use lavashell::prelude::*;
use model::Demo;

include!(concat!(env!("OUT_DIR"), "/copy.rs"));

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
