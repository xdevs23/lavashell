// SPDX-License-Identifier: GPL-3.0-only
use crate::layouts::Layout;
use lavashell_core::prelude::NonNegativeDp;
use lavashell_core::scene::Icon;

#[derive(Clone, Debug, PartialEq)]
pub struct KeyboardSettings {
    pub layouts: Vec<&'static Layout>,
    pub text: KeyboardText,
    pub icons: KeyboardIcons,
    pub panel_height: NonNegativeDp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyboardIcons {
    pub search: &'static Icon,
    pub done: &'static Icon,
    pub next: &'static Icon,
    pub go: &'static Icon,
    pub send: &'static Icon,
    pub enter: &'static Icon,
    pub shift: &'static Icon,
    pub caps_lock: &'static Icon,
    pub delete: &'static Icon,
    pub globe: &'static Icon,
    pub hide: &'static Icon,
    pub active_layout: &'static Icon,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardText {
    pub keyboard: String,
    pub layouts: String,
    pub more_letters: String,
    pub search: String,
    pub done: String,
    pub next: String,
    pub go: String,
    pub send: String,
    pub enter: String,
    pub symbols_label: String,
    pub letters_label: String,
    pub more_symbols_label: String,
    pub symbols: String,
    pub letters: String,
    pub more_symbols: String,
    pub shift: String,
    pub delete: String,
    pub globe: String,
    pub hide: String,
    pub space: String,
}
