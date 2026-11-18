// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod error;
pub mod model;
mod palette;
mod tag;
pub mod ui;

pub use error::Error;
pub use tag::Tag;

use lavashell::prelude::*;
use lavashell_example_support::size_window;
use model::Demo;

include!(concat!(env!("OUT_DIR"), "/copy.rs"));

pub const APP_ID: &str = "lavashell.materials";

pub fn app(app: App, demo: &Demo, size: Option<Size>, fullscreen: bool) -> Result<App, Error> {
    let mut app = if demo.follows_output() {
        let state = demo.clone();
        app.output_observer(move |_, info| state.output.set(Some(info)))
    } else {
        app
    };
    let window = Window::new(copy(CopyKey::Title), APP_ID)
        .fullscreen(fullscreen)
        .output(demo.request.get());
    let window = size_window(window, size);
    let shown = demo.clone();
    app.window(window, move |u| ui::compose(u, &shown))?;
    Ok(app)
}
