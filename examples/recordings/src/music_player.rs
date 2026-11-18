// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, write_screenshot};
use lavashell::prelude::Size;
use lavashell_example_support::ExampleArguments;
use lavashell_music_player::{APP_ID, build_window_app};
use std::path::Path;

pub fn screenshot(
    example: &ExampleArguments,
    size: Option<Size>,
    path: &Path,
) -> Result<(), Error> {
    let app = build_window_app(example, size)?;
    write_screenshot(app, APP_ID, path)
}
