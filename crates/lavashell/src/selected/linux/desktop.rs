// SPDX-License-Identifier: GPL-3.0-only
use crate::RunError;
use lavashell_runtime::App;
use lavashell_store::Store;

include!(concat!(env!("OUT_DIR"), "/registrations.rs"));

pub type Platform = lavashell_desktop::DesktopPlatform<lavashell_wayland::Wayland>;

pub fn run(app: App) -> Result<(), RunError> {
    let mut store = Store::default();
    register_all(&mut store);
    lavashell_runtime::run::<Platform>(app, store)
}
