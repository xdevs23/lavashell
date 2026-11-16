// SPDX-License-Identifier: GPL-3.0-only
use crate::sctk;
use lavashell_core::keyboard::{Key, KeyStroke};
use lavashell_core::platform::KeyRepeat;
use lavashell_platform::{DeviceEvent, KeyAction, KeyboardDevice};
use sctk::seat::keyboard::{KeyEvent, Keysym, RepeatInfo};
use std::time::Duration;

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

pub(crate) fn key_event(
    keyboard: KeyboardDevice,
    event: KeyEvent,
    action: KeyAction,
) -> DeviceEvent {
    DeviceEvent::Key {
        keyboard,
        code: event.raw_code,
        stroke: stroke(event),
        action,
    }
}

pub(crate) fn key_repeat(info: RepeatInfo) -> Option<KeyRepeat> {
    match info {
        RepeatInfo::Repeat { rate, delay } => KeyRepeat::new(
            Duration::from_millis(delay.into()),
            Duration::from_secs(1) / rate.get(),
        ),
        RepeatInfo::Disable => None,
    }
}

#[cfg(test)]
mod tests;
