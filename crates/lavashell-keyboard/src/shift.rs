// SPDX-License-Identifier: GPL-3.0-only
use crate::board::{Key, Page};
use lavashell_core::prelude::{Capitalization, FieldText, TextSession};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shift {
    Off,
    Once,
    Locked,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ShiftChoice {
    shift: Shift,
    text: FieldText,
}

impl ShiftChoice {
    pub fn new(shift: Shift, session: &TextSession) -> Self {
        Self {
            shift,
            text: session.text.clone(),
        }
    }
}

pub(crate) fn resolve(
    choice: Option<&ShiftChoice>,
    session: &TextSession,
    automatic: bool,
) -> Shift {
    match choice {
        Some(choice) if choice.shift != Shift::Off || choice.text == session.text => choice.shift,
        _ if automatic && capital_starts(session) => Shift::Once,
        _ => Shift::Off,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TypingState {
    pub shift: Shift,
    pub page: Page,
    pub double_tap: bool,
}

impl TypingState {
    pub fn after_release(self, key: Key, typed: Option<&str>) -> Self {
        let shift = self.chosen_shift(key, typed).unwrap_or(self.shift);
        let page = match key {
            Key::Page(page) => page,
            _ => self.page,
        };
        Self {
            shift,
            page,
            double_tap: key == Key::Shift && shift != Shift::Locked,
        }
    }

    fn tap_count(self) -> TapCount {
        if self.double_tap {
            TapCount::Double
        } else {
            TapCount::Single
        }
    }

    pub fn chosen_shift(self, key: Key, typed: Option<&str>) -> Option<Shift> {
        let letter = typed.is_some_and(|text| text.chars().any(char::is_alphabetic));
        match key {
            Key::Shift => Some(tapped(self.shift, self.tap_count())),
            _ if letter && self.page == Page::Letters && self.shift != Shift::Locked => {
                Some(Shift::Off)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TapCount {
    Single,
    Double,
}

fn tapped(shift: Shift, taps: TapCount) -> Shift {
    match (shift, taps) {
        (_, TapCount::Double) => Shift::Locked,
        (Shift::Off, TapCount::Single) => Shift::Once,
        (Shift::Once | Shift::Locked, TapCount::Single) => Shift::Off,
    }
}

fn capital_starts(session: &TextSession) -> bool {
    let FieldText::BeforeCursor(before) = &session.text else {
        return false;
    };
    match session.hints.capitalization {
        Capitalization::None => false,
        Capitalization::Characters => true,
        Capitalization::Sentences | Capitalization::Words => {
            before.chars().all(char::is_whitespace)
        }
    }
}
