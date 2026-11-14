// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod error;
pub mod model;
mod theme;
pub mod ui;

pub use error::Error;

use lavashell::core::animation::AnimationError;
use lavashell::prelude::*;
use model::Demo;

include!(concat!(env!("OUT_DIR"), "/copy.rs"));

/// `app` with `demo` in a window of `size`, which covers the screen when `fullscreen` is set. Before each frame, the
/// autoplay of `demo` advances by one step. A step that fails goes to `failed`.
///
/// # Errors
/// Returns an error when the window cannot be added to `app`.
pub fn app(
    app: App,
    demo: &Demo,
    size: Size,
    fullscreen: bool,
    mut failed: impl FnMut(AnimationError) + 'static,
) -> Result<App, Error> {
    let stepped = demo.clone();
    let mut app = app.on_frame(move |frame| {
        if let Err(error) = stepped.step(frame) {
            failed(error);
        }
    });
    let shown = demo.clone();
    app.window(
        Window::new(copy(CopyKey::Title), "lavashell.motion")
            .size(size.width, size.height)
            .fullscreen(fullscreen),
        move |ui| ui::compose(ui, &shown),
    )?;
    Ok(app)
}
