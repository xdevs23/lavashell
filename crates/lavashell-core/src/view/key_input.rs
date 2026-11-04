// SPDX-License-Identifier: GPL-3.0-only
use super::InputEvent;
use crate::keyboard::{Key, KeyStroke, Shortcut};
use crate::ui::InputModifiers;

impl InputEvent {
    /// The input of a press of `stroke` while `modifiers` are held: a shortcut while Control or Alt is held, a named
    /// key, or else the text typed by the key. Text that contains a control character returns `None`.
    #[must_use]
    pub fn from_key_press(stroke: KeyStroke, modifiers: InputModifiers) -> Option<Self> {
        let InputModifiers {
            control,
            shift,
            alt,
        } = modifiers;
        if control || alt {
            let key = match stroke.key? {
                Key::Character(character) => Key::Character(character.to_ascii_lowercase()),
                named => named,
            };
            return Some(Self::Shortcut(Shortcut {
                key,
                control,
                shift,
                alt,
            }));
        }
        match stroke.key {
            Some(Key::Character(_)) | None => (stroke.text)
                .filter(|text| !text.chars().any(char::is_control))
                .map(Self::Text),
            Some(key) => Some(Self::Key { key, shift }),
        }
    }

    /// The input of a release of `stroke`: Enter and Space report their release. Other keys return `None`.
    #[must_use]
    pub fn from_key_release(stroke: &KeyStroke) -> Option<Self> {
        (stroke.key)
            .filter(|key| matches!(key, Key::Enter | Key::Space))
            .map(Self::KeyRelease)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTROL: InputModifiers = InputModifiers {
        control: true,
        shift: false,
        alt: false,
    };

    fn stroke(key: Option<Key>, text: &str) -> KeyStroke {
        KeyStroke {
            key,
            text: Some(text.to_owned()),
        }
    }

    #[test]
    fn a_held_control_or_alt_turns_a_press_into_a_shortcut_with_a_lowercase_character() {
        let shortcut = InputEvent::from_key_press(stroke(Some(Key::Character('C')), "C"), CONTROL);
        assert_eq!(
            shortcut,
            Some(InputEvent::Shortcut(Shortcut::control(Key::Character('c'))))
        );
        assert_eq!(InputEvent::from_key_press(stroke(None, "c"), CONTROL), None);
    }

    #[test]
    fn a_named_key_reports_itself_and_any_other_key_types_its_text() {
        let shift = InputModifiers {
            shift: true,
            ..InputModifiers::default()
        };
        let pressed = |key, text| InputEvent::from_key_press(stroke(key, text), shift);
        assert_eq!(
            pressed(Some(Key::Tab), "\t"),
            Some(InputEvent::Key {
                key: Key::Tab,
                shift: true
            })
        );
        assert_eq!(
            pressed(Some(Key::Character('A')), "A"),
            Some(InputEvent::Text("A".to_owned()))
        );
        assert_eq!(pressed(None, "\u{7}"), None);
    }

    #[test]
    fn only_enter_and_space_report_their_release() {
        let released = |key| InputEvent::from_key_release(&stroke(Some(key), ""));
        assert_eq!(
            released(Key::Space),
            Some(InputEvent::KeyRelease(Key::Space))
        );
        assert_eq!(released(Key::Character('a')), None);
    }
}
