// SPDX-License-Identifier: GPL-3.0-only
use super::device::keyboard_device;
use crate::translator::Translator;
use lavashell_core::keyboard::{Key, KeyStroke};
use lavashell_core::platform::KeyRepeat;
use lavashell_platform::{DeviceEvent, KeyAction, KeyboardDevice};
use sctk::reexports::client::protocol::wl_keyboard;
use sctk::seat::keyboard::{KeyEvent, Keysym, RepeatInfo};
use smithay_client_toolkit as sctk;
use std::time::Duration;

/// The key resolved by the keymap for `event`: a named key, or else the character of its keysym, with the text typed
/// by it.
fn stroke(event: KeyEvent) -> KeyStroke {
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

/// The event for the key of `event` on `keyboard`, which did `action`.
fn key_event(keyboard: KeyboardDevice, event: KeyEvent, action: KeyAction) -> DeviceEvent {
    DeviceEvent::Key {
        keyboard,
        code: event.raw_code,
        stroke: stroke(event),
        action,
    }
}

/// How a held key repeats as `info` says, or `None` where it does not repeat.
pub(super) fn key_repeat(info: RepeatInfo) -> Option<KeyRepeat> {
    match info {
        RepeatInfo::Repeat { rate, delay } => KeyRepeat::new(
            Duration::from_millis(delay.into()),
            Duration::from_secs(1) / rate.get(),
        ),
        RepeatInfo::Disable => None,
    }
}

impl Translator {
    pub(super) fn report_key(
        &self,
        keyboard: &wl_keyboard::WlKeyboard,
        event: KeyEvent,
        action: KeyAction,
    ) {
        self.links
            .report(key_event(keyboard_device(keyboard), event, action));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;
    use std::num::NonZeroU32;

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

    #[test]
    fn a_repeated_key_carries_its_evdev_code_and_its_repeat() {
        let event = KeyEvent {
            time: 0,
            raw_code: 30,
            keysym: Keysym::a,
            utf8: Some("a".to_owned()),
        };
        let expected = DeviceEvent::Key {
            keyboard: KeyboardDevice(3),
            code: 30,
            stroke: KeyStroke {
                key: Some(Key::Character('a')),
                text: Some("a".to_owned()),
            },
            action: KeyAction::Repeated,
        };
        let translated = key_event(KeyboardDevice(3), event, KeyAction::Repeated);
        assert_eq!(translated, expected);
    }

    #[test]
    fn the_repeat_rate_becomes_the_interval_between_two_repeats() -> Result<(), TestError> {
        let info = RepeatInfo::Repeat {
            rate: NonZeroU32::try_from(25_u32)?,
            delay: 600,
        };
        let expected = KeyRepeat::new(Duration::from_millis(600), Duration::from_millis(40));
        assert_eq!(key_repeat(info), expected);
        assert_eq!(key_repeat(RepeatInfo::Disable), None);
        Ok(())
    }
}
