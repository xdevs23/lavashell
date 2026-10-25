// SPDX-License-Identifier: GPL-3.0-only
//! The storage of text typed into a protected field, such as a password. Scene strings and the editor state get
//! only masks, one mask character for each typed character.

mod presentation;

use crate::reactive::State;
pub use presentation::PrivateText;
use std::{cell::RefCell, iter, ops::Range, rc::Rc};
use zeroize::Zeroizing;

#[cfg(test)]
mod tests;

/// The text of a protected field, moved out to the code that uses the credential. The type implements neither
/// `Clone`, `Debug`, `Display` nor serialization. Dropping it erases its allocation. Code that borrows the plain
/// text through `expose` must not log or store it.
#[derive(Default)]
pub struct SecretText {
    /// Each edit builds a new string and erases the string that it replaces. No byte of an earlier text then
    /// remains in a freed allocation.
    text: Zeroizing<String>,
}

impl SecretText {
    /// Lends the plain text to `read` and returns the result of `read`. Call this only for the operation that uses
    /// the credential.
    pub fn expose<R>(&self, read: impl FnOnce(&str) -> R) -> R {
        read(&self.text)
    }

    /// Replaces the characters in `scalars` with `input`, whose line breaks become spaces, and
    /// returns how many characters it inserted.
    fn replace(&mut self, scalars: Range<usize>, input: &str) -> usize {
        let text = self.text.as_str();
        let boundary = |scalar| {
            (text.char_indices())
                .nth(scalar)
                .map_or(text.len(), |(index, _)| index)
        };
        let start = boundary(scalars.start);
        let end = boundary(scalars.end).max(start);
        let normalized = || {
            input
                .chars()
                .map(|c| if matches!(c, '\r' | '\n') { ' ' } else { c })
        };
        let inserted = normalized().map(char::len_utf8).sum::<usize>();
        let mut replaced = String::with_capacity(text.len() - (end - start) + inserted);
        replaced.push_str(&text[..start]);
        replaced.extend(normalized());
        replaced.push_str(&text[end..]);
        self.text = Zeroizing::new(replaced);
        normalized().count()
    }

    fn copy(&self) -> Self {
        Self {
            text: Zeroizing::new(self.text.as_str().to_owned()),
        }
    }

    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// A handle to the text of a protected field, shared by the field and the application. The field shows masks
/// unless the text is revealed. `take` moves the text out as a `SecretText`.
#[derive(Clone)]
pub struct ProtectedText {
    value: Rc<RefCell<SecretText>>,
    /// Set after each edit, also after an edit that leaves the length of the text unchanged, so that every reader
    /// of the text is notified.
    edits: State<()>,
    revealed: State<bool>,
    presentation: Rc<RefCell<Option<PrivateText>>>,
}

impl Default for ProtectedText {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtectedText {
    #[must_use]
    pub fn new() -> Self {
        Self {
            value: Rc::new(RefCell::new(SecretText::default())),
            edits: State::new(()),
            revealed: State::new(false),
            presentation: Rc::new(RefCell::new(None)),
        }
    }

    /// The text as masks, one `mask` for each character.
    #[must_use]
    pub fn masked(&self, mask: char) -> String {
        self.edits.track();
        self.value.borrow().expose(|text| mask_text(text, mask))
    }

    /// Shows or hides the characters of the text in the field. In both cases, the clipboard and accessibility
    /// services do not get the text.
    pub fn set_revealed(&self, revealed: bool) {
        self.revealed.set(revealed);
        if !revealed {
            *self.presentation.borrow_mut() = None;
        }
    }

    pub(crate) fn presentation(&self, mask: char) -> Option<PrivateText> {
        self.edits.track();
        if !self.revealed.get() || self.value.borrow().is_empty() {
            return None;
        }
        let mut shown = self.presentation.borrow_mut();
        if shown.as_ref().is_none_or(|private| private.mask != mask) {
            *shown = Some(PrivateText::snapshot(&self.value.borrow(), mask));
        }
        shown.clone()
    }

    /// Clears the field and moves its text out, without a copy, to the code that uses the credential.
    #[must_use]
    pub fn take(&self) -> SecretText {
        let result = std::mem::take(&mut *self.value.borrow_mut());
        self.changed();
        result
    }

    pub fn clear(&self) {
        *self.value.borrow_mut() = SecretText::default();
        self.changed();
    }

    pub(crate) fn replace(&self, scalars: Range<usize>, input: &str) -> usize {
        let inserted = self.value.borrow_mut().replace(scalars, input);
        self.changed();
        inserted
    }

    fn changed(&self) {
        *self.presentation.borrow_mut() = None;
        self.edits.set(());
    }
}

/// A protected text in a field, shown as `mask` once for each of its characters.
#[derive(Clone)]
pub struct ProtectedInput {
    pub(crate) text: ProtectedText,
    mask: char,
}

impl ProtectedInput {
    pub(crate) fn new(text: ProtectedText, mask: char) -> Self {
        Self { text, mask }
    }

    pub(crate) fn masked(&self) -> String {
        self.text.masked(self.mask)
    }

    pub(crate) fn presentation(&self) -> Option<PrivateText> {
        self.text.presentation(self.mask)
    }

    /// The index of the character of the masked text that starts at the byte `offset`.
    pub(crate) fn char_at(&self, offset: usize) -> usize {
        offset / self.mask.len_utf8()
    }

    /// The byte offset in the masked text at which the character at `index` starts.
    pub(crate) fn offset_of(&self, index: usize) -> usize {
        index * self.mask.len_utf8()
    }
}

fn mask_text(text: &str, mask: char) -> String {
    iter::repeat_n(mask, text.chars().count()).collect()
}
