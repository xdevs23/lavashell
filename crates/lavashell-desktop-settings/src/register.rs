// SPDX-License-Identifier: GPL-3.0-only
use crate::service::DesktopSettingsService;
use lavashell_store::Store;

pub fn register(store: &mut Store) {
    lavashell_desktop::plug_in(store, DesktopSettingsService::from_layers);
}
