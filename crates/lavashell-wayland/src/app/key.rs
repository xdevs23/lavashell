// SPDX-License-Identifier: GPL-3.0-only
use super::{InputEvent, Key, KeyEvent, Keysym, Platform, Shortcut, wl_keyboard};

impl Platform {
    pub(super) fn key(&mut self, keyboard: &wl_keyboard::WlKeyboard, event: KeyEvent) {
        let Some(seat) = self.keyboard_seat(keyboard) else {
            return;
        };
        let Some(index) = seat.focus else { return };
        let shift = seat.modifiers.shift;
        let key = match event.keysym {
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
        let input = if seat.modifiers.ctrl || seat.modifiers.alt {
            let key = key.or_else(|| {
                event
                    .keysym
                    .key_char()
                    .map(|ch| Key::Character(ch.to_ascii_lowercase()))
            });
            key.map(|key| {
                InputEvent::Shortcut(Shortcut {
                    key,
                    control: seat.modifiers.ctrl,
                    shift,
                    alt: seat.modifiers.alt,
                })
            })
        } else if let Some(key) = key {
            Some(InputEvent::Key { key, shift })
        } else {
            event
                .utf8
                .filter(|text| !text.chars().any(char::is_control))
                .map(InputEvent::Text)
        };
        let Some(input) = input else {
            return;
        };
        let seat = seat.seat.clone();
        let input = if let Some(filter) = &mut self.config.input_filter {
            filter(index, input)
        } else {
            Some(input)
        };
        let Some(input) = input else {
            return;
        };
        if let InputEvent::Shortcut(Shortcut {
            key,
            control: true,
            alt: false,
            ..
        }) = &input
        {
            let field = self.surfaces[index].view.editing_target().is_some();
            if field && *key == Key::Character('v') {
                self.paste_selection(index, &seat);
                return;
            }
            if *key == Key::Character('x') {
                self.cut_text(index);
                return;
            }
            if *key == Key::Character('c')
                && let Some(text) = self.surfaces[index].view.copy_selection(false)
            {
                self.copy_text(&text);
                return;
            }
        }
        self.surfaces[index].view.input(input);
    }
}
