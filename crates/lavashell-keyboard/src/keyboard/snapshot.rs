// SPDX-License-Identifier: GPL-3.0-only
use super::{ComposingView, Highlight};
use crate::board::{Board, Key, Page};
use crate::facts::Facts;
use crate::layouts::Layout;
use crate::popup::{FurtherLetters, Popup};
use crate::press::Preview;
use crate::settings::KeyboardSettings;
use crate::shift::{Shift, TypingState};
use lavashell_core::prelude::TextSession;
use std::rc::Rc;

/// The state of a shown keyboard, read all at once so that one composition or one input uses
/// consistent values.
pub(crate) struct Snapshot {
    pub view: ComposingView,
    pub session: TextSession,
    pub settings: Rc<KeyboardSettings>,
    pub facts: Rc<Facts>,
    /// The layouts that the globe key switches between, in order.
    pub layouts: Vec<&'static Layout>,
    pub layout: &'static Layout,
    pub page: Page,
    pub shift: Shift,
    pub board: Board,
    pub highlight: Highlight,
    pub popup: Option<Popup>,
    pub previews: Vec<Preview>,
    /// Whether a tap on shift is the second tap of a double tap.
    pub double_tap: bool,
}

impl Snapshot {
    /// `letter` as a capital while shift is on, otherwise `letter` unchanged. Shift applies only on
    /// the page of letters, and only to a letter whose capital is one character. A letter such as
    /// ß, whose capital is SS, does not change.
    pub fn cased(&self, letter: char) -> char {
        cased(letter, self.shift, self.page)
    }

    pub fn character(&self, key: Key) -> Option<String> {
        self.character_on(key, self.typing())
    }

    pub fn typing(&self) -> TypingState {
        TypingState {
            shift: self.shift,
            page: self.page,
            double_tap: self.double_tap,
        }
    }

    fn character_on(&self, key: Key, typing: TypingState) -> Option<String> {
        match key {
            Key::Char(letter) => Some(cased(letter, typing.shift, typing.page).to_string()),
            Key::Currency => Some(self.layout.currency().to_string()),
            _ => None,
        }
    }

    /// The character typed by `key` when it is released after the keys in `before`, which were
    /// pressed earlier and are still held. Releasing those keys first changes shift and the page,
    /// and the character follows that changed state.
    pub fn character_after(&self, before: &[Key], key: Key) -> Option<String> {
        let typing = (before.iter()).fold(self.typing(), |typing, &held| {
            typing.after_release(held, self.character_on(held, typing).as_deref())
        });
        self.character_on(key, typing)
    }

    pub fn popup_held_by(&self, serial: u64) -> Option<&Popup> {
        (self.popup.as_ref()).filter(|popup| popup.held_by(serial))
    }

    /// The further letters listed for `letter` by the shown layout and then by each configured
    /// layout, without the letters that have a key on the shown layout. The list is empty outside
    /// the page of letters. A letter listed by several layouts comes once for each of them.
    pub fn listed_letters(&self, letter: char) -> impl Iterator<Item = char> + '_ {
        let layouts = std::iter::once(self.layout).chain(self.layouts.iter().copied());
        layouts
            .filter(|_| self.page == Page::Letters)
            .filter_map(move |layout| layout.further_letters_of(letter))
            .flat_map(str::chars)
            .filter(|listed| !self.layout.has_key(*listed))
    }

    /// The letters offered by a long press on `letter`: the listed letters, each once.
    pub fn further_letters(&self, letter: char) -> FurtherLetters {
        FurtherLetters::new(self.listed_letters(letter))
    }
}

/// `letter` as a capital when `shift` is on, `page` is the page of letters and the capital is one
/// character, otherwise `letter` unchanged.
fn cased(letter: char, shift: Shift, page: Page) -> char {
    if page != Page::Letters || shift == Shift::Off {
        return letter;
    }
    let mut capital = letter.to_uppercase();
    match (capital.next(), capital.next()) {
        (Some(capital), None) => capital,
        _ => letter,
    }
}
