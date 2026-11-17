// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::keyboard::Key;

// https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/include/uapi/linux/input-event-codes.h?h=v7.2
pub(crate) fn convert_to_linux_key_code(key: Key) -> Option<u32> {
    match key {
        Key::Escape => Some(1),
        Key::Backspace => Some(14),
        Key::Tab => Some(15),
        Key::Enter => Some(28),
        Key::Space => Some(57),
        Key::Home => Some(102),
        Key::Up => Some(103),
        Key::PageUp => Some(104),
        Key::Left => Some(105),
        Key::Right => Some(106),
        Key::End => Some(107),
        Key::Down => Some(108),
        Key::PageDown => Some(109),
        Key::Delete => Some(111),
        Key::Character(_) => None,
    }
}
