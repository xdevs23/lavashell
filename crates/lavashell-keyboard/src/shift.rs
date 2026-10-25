// SPDX-License-Identifier: GPL-3.0-only
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

/// The shift state after a tap on the shift key in the state `shift`. A `double` tap turns caps lock
/// on. A single tap turns shift on for one letter when shift is off, and turns shift off otherwise.
pub(crate) fn tapped(shift: Shift, double: bool) -> Shift {
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
