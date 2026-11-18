// SPDX-License-Identifier: GPL-3.0-only
use crate::sctk;
use lavashell_core::view::Cursor;
use lavashell_core::window::ResizeEdge;
use lavashell_platform::{Button, DeviceEvent, PressState};
use sctk::seat::pointer::CursorIcon;

pub(crate) fn cursor_icon(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Text => CursorIcon::Text,
        Cursor::Resize(ResizeEdge::Top) => CursorIcon::NResize,
        Cursor::Resize(ResizeEdge::TopRight) => CursorIcon::NeResize,
        Cursor::Resize(ResizeEdge::Right) => CursorIcon::EResize,
        Cursor::Resize(ResizeEdge::BottomRight) => CursorIcon::SeResize,
        Cursor::Resize(ResizeEdge::Bottom) => CursorIcon::SResize,
        Cursor::Resize(ResizeEdge::BottomLeft) => CursorIcon::SwResize,
        Cursor::Resize(ResizeEdge::Left) => CursorIcon::WResize,
        Cursor::Resize(ResizeEdge::TopLeft) => CursorIcon::NwResize,
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

#[cfg(test)]
mod tests;
