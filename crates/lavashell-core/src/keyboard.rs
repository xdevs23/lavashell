// SPDX-License-Identifier: GPL-3.0-only
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Key {
    Tab,
    Enter,
    Space,
    Backspace,
    Delete,
    Escape,
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Character(char),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Shortcut {
    pub key: Key,
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Shortcut {
    #[must_use]
    pub const fn key(key: Key) -> Self {
        Self {
            key,
            control: false,
            shift: false,
            alt: false,
        }
    }

    #[must_use]
    pub const fn control(key: Key) -> Self {
        Self {
            key,
            control: true,
            shift: false,
            alt: false,
        }
    }

    #[must_use]
    pub const fn shift(mut self) -> Self {
        self.shift = true;
        self
    }
}
