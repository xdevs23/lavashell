// SPDX-License-Identifier: GPL-3.0-only
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
