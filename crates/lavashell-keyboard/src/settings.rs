// SPDX-License-Identifier: GPL-3.0-only
use crate::layouts::Layout;
use lavashell_core::prelude::NonNegativeDp;
use lavashell_core::scene::Icon;

/// The settings chosen by the app for the keyboard, passed to `Keyboard::new` inside a `State`. A
/// change takes effect the next time the keyboard is composed. The keyboard reads the settings made
/// by the user on the platform from the view instead: the preferences for automatic capitals and
/// key previews, the timing of a long press, a double tap and a held key, and the languages of the
/// user.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyboardSettings {
    /// The layouts available in the app. The keyboard offers those whose language is a language of
    /// the user, in the order of those languages. When no layout matches, it offers all of them in
    /// this order. The globe key switches to the next offered layout and appears only when two or
    /// more are offered. While the shown layout is not offered, the keyboard shows the first offered
    /// layout. A long press on a letter shows the accented letters listed under it by the shown
    /// layout and by each offered layout.
    pub layouts: Vec<&'static Layout>,
    pub text: KeyboardText,
    pub icons: KeyboardIcons,
    /// The height of the panel at rest. The keys are as tall as the four rows leave room for after
    /// the padding and the gaps of the panel.
    pub panel_height: NonNegativeDp,
}

/// The icons drawn on the keys and in the list of layouts. The action key shows the icon for the
/// action of the focused field, or `enter` when the field has no action. The shift key shows `shift`
/// outlined while shift is off, `shift` filled while the next letter is a capital and `caps_lock`
/// while caps lock is on. The list of layouts marks the shown layout with `active_layout`.
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

/// The words on the keys and the names that assistive technology reads for them, in the language
/// of the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardText {
    /// The name of the panel read by assistive technology.
    pub keyboard: String,
    /// The heading of the list of layouts.
    pub layouts: String,
    /// The name of the popup with the accented letters shown on a long press.
    pub more_letters: String,
    pub search: String,
    pub done: String,
    pub next: String,
    pub go: String,
    pub send: String,
    /// The word on the action key of a field without an action.
    pub enter: String,
    /// The label on the key that switches to the first page of symbols. `letters_label` and
    /// `more_symbols_label` are the labels on the keys that switch to the letters and to the second
    /// page of symbols.
    pub symbols_label: String,
    pub letters_label: String,
    pub more_symbols_label: String,
    /// The name read by assistive technology for the key that switches to the first page of
    /// symbols. `letters` and `more_symbols` are the names of the other two keys that switch pages.
    pub symbols: String,
    pub letters: String,
    pub more_symbols: String,
    pub shift: String,
    pub delete: String,
    pub globe: String,
    pub hide: String,
    pub space: String,
}
