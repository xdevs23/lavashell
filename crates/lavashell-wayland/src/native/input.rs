// SPDX-License-Identifier: GPL-3.0-only
use crate::sctk;
use lavashell_core::view::Cursor;
use lavashell_platform::{Button, DeviceEvent, PressState};
use sctk::seat::pointer::CursorIcon;

pub(crate) fn cursor_icon(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Text => CursorIcon::Text,
    }
}

pub(crate) fn is_primary_press(event: &DeviceEvent) -> bool {
    matches!(
        event,
        DeviceEvent::Button {
            button: Button::Primary,
            state: PressState::Pressed,
            ..
        }
    )
}
