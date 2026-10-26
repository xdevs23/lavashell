// SPDX-License-Identifier: GPL-3.0-only
use crate::board::{Key, Page};
use lavashell_core::prelude::{Capitalization, FieldText, TextSession};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shift {
    Off,
    /// A capital for the next letter.
    Once,
    /// Capitals until shift is tapped again.
    Locked,
}

/// A shift state set by the shift key or by typing a letter, together with the text before the
/// cursor at that moment. Caps lock lasts while the field has focus. A capital for the next letter
/// lasts until a letter is typed. Shift turned off lasts until the text changes. After that, the
/// capitalization of the field applies again.
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

/// The shift state for `session`. When `choice` no longer applies, shift is on for one letter if
/// `automatic` is true and the capitalization of the field requires a capital at the cursor.
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

/// The state that the release of a key changes apart from the text: shift, the page, and whether a
/// tap on shift is the second tap of a double tap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TypingState {
    pub shift: Shift,
    pub page: Page,
    pub double_tap: bool,
}

impl TypingState {
    /// The state after the release of `key`, which typed `typed`. A tap on shift soon after a release
    /// of shift that left caps lock off is a double tap.
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

    /// The shift state that the release of `key`, which typed `typed`, chooses, or `None` when the
    /// release leaves shift as it is. On the page of letters, typing a letter turns shift off, unless
    /// caps lock is on.
    pub fn chosen_shift(self, key: Key, typed: Option<&str>) -> Option<Shift> {
        let letter = typed.is_some_and(|text| text.chars().any(char::is_alphabetic));
        match key {
            Key::Shift => Some(tapped(self.shift, self.double_tap)),
            _ if letter && self.page == Page::Letters && self.shift != Shift::Locked => {
                Some(Shift::Off)
            }
            _ => None,
        }
    }
}

/// The shift state after a tap on the shift key in the state `shift`. A `double` tap turns caps lock
/// on. A single tap turns shift on for one letter when shift is off, and turns shift off otherwise.
fn tapped(shift: Shift, double: bool) -> Shift {
    match (shift, double) {
        (_, true) => Shift::Locked,
        (Shift::Off, false) => Shift::Once,
        (Shift::Once | Shift::Locked, false) => Shift::Off,
    }
}

/// Whether the capitalization of the field requires a capital at the cursor. With `Characters`,
/// every letter is a capital. With `Sentences` or `Words`, only the first letter of the text is a
/// capital, while only whitespace is before the cursor. A field whose text is protected or unknown
/// gets no capital.
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
