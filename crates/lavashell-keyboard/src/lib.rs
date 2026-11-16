// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod board;
mod compose;
mod keyboard;
mod layouts;
mod look;
mod placement;
mod platform_typing_settings;
mod popup;
mod press;
mod settings;
mod shift;
mod stage;
mod style;

pub use compose::UiKeyboard;
pub use keyboard::{Keyboard, Target, TextSink};
pub use layouts::{Currency, Layout, LayoutError, LayoutSpec, ShippedLayouts};
pub use placement::{Dock, Placement, Side};
pub use settings::{KeyboardIcons, KeyboardSettings, KeyboardText};
pub use stage::IslandReturn;
pub use style::{KEYBOARD_STYLE, KeyboardStyle};

#[cfg(test)]
mod tests;
