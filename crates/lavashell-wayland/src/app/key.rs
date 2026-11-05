// SPDX-License-Identifier: GPL-3.0-only
use super::Platform;
use super::device::keyboard_device;
use crate::clipboard::{ClipboardShortcut, clipboard_shortcut};
use lavashell_core::keyboard::{Key, KeyStroke, Shortcut};
use lavashell_core::view::InputEvent;
use lavashell_platform::{DeviceEvent, PressState};
use lavashell_runtime::RoutedInput;
use sctk::reexports::client::protocol::{wl_keyboard, wl_seat};
use sctk::seat::keyboard::{KeyEvent, Keysym};
use smithay_client_toolkit as sctk;

/// The key resolved by the keymap for `event`: a named key, or else the character of its keysym, with the text typed
/// by it.
pub(super) fn stroke(event: KeyEvent) -> KeyStroke {
    let named = match event.keysym {
        Keysym::Tab | Keysym::ISO_Left_Tab => Some(Key::Tab),
        Keysym::Return | Keysym::KP_Enter => Some(Key::Enter),
        Keysym::BackSpace => Some(Key::Backspace),
        Keysym::Delete => Some(Key::Delete),
        Keysym::space => Some(Key::Space),
        Keysym::Escape => Some(Key::Escape),
        Keysym::Left => Some(Key::Left),
        Keysym::Right => Some(Key::Right),
        Keysym::Up => Some(Key::Up),
        Keysym::Down => Some(Key::Down),
        Keysym::Page_Up => Some(Key::PageUp),
        Keysym::Page_Down => Some(Key::PageDown),
        Keysym::Home => Some(Key::Home),
        Keysym::End => Some(Key::End),
        _ => None,
    };
    KeyStroke {
        key: named.or_else(|| event.keysym.key_char().map(Key::Character)),
        text: event.utf8,
    }
}

impl Platform {
    /// Routes a pressed key of `keyboard` to the view of the focused surface. Control with C, X or V copies, cuts or
    /// pastes instead.
    pub(super) fn key(&mut self, keyboard: &wl_keyboard::WlKeyboard, event: KeyEvent) {
        let Some(seat) = self.keyboard_seat(keyboard).map(|seat| seat.seat.clone()) else {
            return;
        };
        let pressed = DeviceEvent::Key {
            keyboard: keyboard_device(keyboard),
            stroke: stroke(event),
            state: PressState::Pressed,
        };
        for routed in self.input.route(pressed) {
            if !self.run_clipboard_shortcut(&routed, &seat) {
                self.deliver(routed);
            }
        }
    }

    /// Copies, cuts or pastes text for a shortcut of Control with C, X or V, and returns whether it did. The view
    /// receives the shortcut when there is nothing to copy, cut or paste.
    fn run_clipboard_shortcut(&mut self, routed: &RoutedInput, seat: &wl_seat::WlSeat) -> bool {
        let RoutedInput::Input {
            surface,
            input:
                InputEvent::Shortcut(Shortcut {
                    key,
                    control: true,
                    alt: false,
                    ..
                }),
            ..
        } = *routed
        else {
            return false;
        };
        let Some(shortcut) = self
            .clipboard_view(surface)
            .and_then(|view| clipboard_shortcut(view, key))
        else {
            return false;
        };
        match shortcut {
            ClipboardShortcut::Paste => self.paste_selection(surface, seat),
            ClipboardShortcut::Cut(source, data) => self.cut_text(surface, source, data),
            ClipboardShortcut::Copy(text) => self.copy_text(&text),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(keysym: Keysym, text: Option<&str>) -> KeyStroke {
        stroke(KeyEvent {
            time: 0,
            raw_code: 0,
            keysym,
            utf8: text.map(str::to_owned),
        })
    }

    #[test]
    fn the_keypad_enter_and_the_left_tab_resolve_to_their_named_keys() {
        assert_eq!(resolved(Keysym::KP_Enter, Some("\r")).key, Some(Key::Enter));
        assert_eq!(resolved(Keysym::ISO_Left_Tab, None).key, Some(Key::Tab));
    }

    #[test]
    fn a_printable_keysym_resolves_to_its_character_with_the_typed_text() {
        let expected = KeyStroke {
            key: Some(Key::Character('a')),
            text: Some("a".to_owned()),
        };
        assert_eq!(resolved(Keysym::a, Some("a")), expected);
    }
}
