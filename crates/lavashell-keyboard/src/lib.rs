// SPDX-License-Identifier: GPL-3.0-only
//! An on-screen keyboard that is composed into a view like any other component. The keyboard types
//! either into the focused field of that view or into a field edited by the host, to which it sends
//! each edit as a command.
#![forbid(unsafe_code)]

mod board;
mod compose;
mod facts;
mod keyboard;
mod layouts;
mod look;
mod popup;
mod press;
mod settings;
mod shift;
mod stage;
mod style;

pub use compose::UiKeyboard;
pub use keyboard::{Keyboard, Target, TextSink};
pub use layouts::{ENGLISH, FRENCH, GERMAN, LAYOUTS, Layout, LayoutError, LayoutSpec, PORTUGUESE};
pub use settings::{KeyboardIcons, KeyboardSettings, KeyboardText};
pub use stage::IslandReturn;
pub use style::{KEYBOARD_STYLE, KeyboardStyle};

#[cfg(test)]
mod tests;
