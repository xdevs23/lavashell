// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod error;
pub mod model;
pub mod ui;

pub use error::Error;

use lavashell::prelude::*;
use model::Demo;

include!(concat!(env!("OUT_DIR"), "/copy.rs"));

pub const APP_ID: &str = "lavashell.materials";

/// `app` with `demo` in a window of `size`, or of the size measured for the demo when `size` is absent. The window
/// covers the screen when `fullscreen` is set and asks for the output that `demo` requests. `demo` receives the
/// output that shows the window.
///
/// # Errors
/// Returns an error when the window cannot be added to `app`.
pub fn app(app: App, demo: &Demo, size: Option<Size>, fullscreen: bool) -> Result<App, Error> {
    let state = demo.clone();
    let mut app = app.output_observer(move |_, info| state.output.set(Some(info)));
    let window = Window::new(copy(CopyKey::Title), APP_ID)
        .fullscreen(fullscreen)
        .output(demo.request.get());
    let window = match size {
        Some(size) => window.size(size.width, size.height),
        None => window,
    };
    let shown = demo.clone();
    app.window(window, move |u| ui::compose(u, &shown))?;
    Ok(app)
}
