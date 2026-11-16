// SPDX-License-Identifier: GPL-3.0-only

mod presentation;

use crate::reactive::State;
use crate::rebuilt::BuiltLike;
pub use presentation::PrivateText;
use std::{cell::RefCell, iter, ops::Range, rc::Rc};
use zeroize::Zeroizing;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub struct SecretText {
    text: Zeroizing<String>,
}

impl SecretText {
    pub fn expose<R>(&self, read: impl FnOnce(&str) -> R) -> R {
        read(&self.text)
    }

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

#[derive(Clone)]
pub struct ProtectedText {
    value: Rc<RefCell<SecretText>>,
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

    #[must_use]
    pub fn masked(&self, mask: char) -> String {
        self.edits.track();
        self.value.borrow().expose(|text| mask_text(text, mask))
    }

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

#[derive(Clone)]
pub struct ProtectedInput {
    pub(crate) text: ProtectedText,
    mask: char,
}

impl BuiltLike for ProtectedInput {
    fn is_built_like(&self, earlier: &Self) -> bool {
        self.text.value.is_built_like(&earlier.text.value) && self.mask == earlier.mask
    }
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

    pub(crate) fn char_at(&self, offset: usize) -> usize {
        offset / self.mask.len_utf8()
    }

    pub(crate) fn offset_of(&self, index: usize) -> usize {
        index * self.mask.len_utf8()
    }
}

fn mask_text(text: &str, mask: char) -> String {
    iter::repeat_n(mask, text.chars().count()).collect()
}
