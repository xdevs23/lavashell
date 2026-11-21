// SPDX-License-Identifier: GPL-3.0-only
use crate::RunError;
use lavashell_runtime::App;

pub type Platform = lavashell_wayland::Wayland;

pub fn run(app: App) -> Result<(), RunError> {
    lavashell_runtime::run::<Platform>(app, ())
}
