// SPDX-License-Identifier: GPL-3.0-only
use crate::appearance::AppearanceSource;
use lavashell_store::Store;
use std::sync::Arc;

pub fn register(store: &mut Store) {
    lavashell_desktop_settings::add_source(store, Arc::new(AppearanceSource));
}

#[cfg(test)]
mod tests;
