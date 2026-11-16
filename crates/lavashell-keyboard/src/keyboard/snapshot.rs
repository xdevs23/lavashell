// SPDX-License-Identifier: GPL-3.0-only
use super::{ComposingView, Highlight};
use crate::board::{Board, Key, Page, Typed};
use crate::layouts::Layout;
use crate::platform_typing_settings::PlatformTypingSettings;
use crate::popup::{FurtherLetters, Popup};
use crate::press::{PressSerial, Preview};
use crate::settings::KeyboardSettings;
use crate::shift::{Shift, TypingState};
use lavashell_core::prelude::TextSession;
use std::rc::Rc;

pub(crate) struct Snapshot {
    pub view: ComposingView,
    pub session: TextSession,
    pub settings: Rc<KeyboardSettings>,
    pub platform: Rc<PlatformTypingSettings>,
    pub layouts: Vec<&'static Layout>,
    pub layout: &'static Layout,
    pub page: Page,
    pub shift: Shift,
    pub board: Board,
    pub highlight: Highlight,
    pub popup: Option<Popup>,
    pub previews: Vec<Preview>,
    pub double_tap: bool,
}

impl Snapshot {
    pub fn cased(&self, letter: char) -> char {
        cased(letter, self.shift, self.page)
    }

    pub fn character(&self, key: Key) -> Option<String> {
        self.character_on(key, self.typing())
    }

    pub fn typed(&self, typed: Typed) -> String {
        self.typed_on(typed, self.typing())
    }

    pub fn typing(&self) -> TypingState {
        TypingState {
            shift: self.shift,
            page: self.page,
            double_tap: self.double_tap,
        }
    }

    fn typed_on(&self, typed: Typed, typing: TypingState) -> String {
        match typed {
            Typed::Char(letter) => cased(letter, typing.shift, typing.page).to_string(),
            Typed::Currency => self.layout.currency().symbol().to_string(),
        }
    }

    fn character_on(&self, key: Key, typing: TypingState) -> Option<String> {
        match key {
            Key::Typed(typed) => Some(self.typed_on(typed, typing)),
            _ => None,
        }
    }

    pub fn character_after(&self, before: &[Key], key: Key) -> Option<String> {
        let typing = (before.iter()).fold(self.typing(), |typing, &held| {
            typing.after_release(held, self.character_on(held, typing).as_deref())
        });
        self.character_on(key, typing)
    }

    pub fn popup_held_by(&self, serial: PressSerial) -> Option<&Popup> {
        (self.popup.as_ref()).filter(|popup| popup.held_by(serial))
    }

    pub fn listed_letters(&self, letter: char) -> impl Iterator<Item = char> + '_ {
        let layouts = std::iter::once(self.layout).chain(self.layouts.iter().copied());
        layouts
            .filter(|_| self.page == Page::Letters)
            .filter_map(move |layout| layout.further_letters_of(letter))
            .flat_map(str::chars)
            .filter(|listed| !self.layout.has_key(*listed))
    }

    pub fn further_letters(&self, letter: char) -> FurtherLetters {
        FurtherLetters::new(self.listed_letters(letter))
    }
}

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
