// SPDX-License-Identifier: GPL-3.0-only
use super::ViewActor;
use crate::clipboard::ShortcutOutcome;
use crate::input::RoutedInput;
use crate::observers::{HostKey, KeyboardFocus};
use crate::view_actor::ViewError;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{
    DeviceEvent, KeyAction, KeyboardDevice, PlatformRequest, PressState, WindowControlRequest,
};
use lavashell_renderer::SurfaceControl;

#[derive(Clone, Copy, PartialEq, Eq)]
enum WindowMove {
    Begins,
    NotBegun,
}

impl<C: SurfaceControl + Send + 'static> ViewActor<C> {
    /// Routes `event` to the views. The key observer of the app sees each key before the views, and the focus
    /// observer of the app sees each surface that gains or loses the keyboard focus.
    pub(super) fn device_event(&mut self, event: DeviceEvent) -> Result<(), ViewError> {
        match event {
            DeviceEvent::PointerEnter { surface, .. } => self.pointer_event(event, Some(surface)),
            DeviceEvent::PointerLeave { pointer }
            | DeviceEvent::PointerMotion { pointer, .. }
            | DeviceEvent::Button { pointer, .. }
            | DeviceEvent::Axis { pointer, .. } => {
                let surface = self.input.pointer_surface(pointer);
                self.pointer_event(event, surface)
            }
            DeviceEvent::Key {
                keyboard,
                code,
                action,
                ..
            } => {
                if !self.host_key(keyboard, code, action) {
                    self.key(event, action)?;
                }
                Ok(())
            }
            DeviceEvent::KeyboardFocus { keyboard, surface } => {
                let left = self.input.keyboard_focus(keyboard);
                if let Some(focused) = surface {
                    self.notify_keyboard_focus(focused, true);
                }
                self.route(event)?;
                if surface.is_none()
                    && let Some(left) = left
                {
                    self.notify_keyboard_focus(left, false);
                }
                self.update_window_focus()
            }
            DeviceEvent::PointerRemoved(_) | DeviceEvent::KeyboardRemoved(_) => {
                self.route(event)?;
                self.update_window_focus()
            }
            DeviceEvent::Modifiers { .. }
            | DeviceEvent::TouchDown { .. }
            | DeviceEvent::TouchMotion(_)
            | DeviceEvent::TouchUp(_)
            | DeviceEvent::TouchCancel(_) => self.route(event),
        }
    }

    fn route(&mut self, event: DeviceEvent) -> Result<(), ViewError> {
        for routed in self.input.route(event) {
            let _moves_start_only_from_pointer_input = self.deliver(routed)?;
        }
        Ok(())
    }

    /// Routes `event` of a pointer over `surface`. A motion that starts a drag of the app is not delivered to the
    /// view. A press starts a drag gesture once the view received it, unless it started a move of the window.
    fn pointer_event(
        &mut self,
        event: DeviceEvent,
        surface: Option<SurfaceId>,
    ) -> Result<(), ViewError> {
        let Some(surface) = surface else {
            return self.route(event);
        };
        let press = button_press(&event);
        let drags = &mut self.transfers.drags;
        let dragging = press.is_none() && drags.track_gesture(surface, &event, &mut self.records);
        let mut window_move = WindowMove::NotBegun;
        for routed in self.input.route(event) {
            if (!dragging || matches!(routed, RoutedInput::Cancel { .. }))
                && self.deliver(routed)? == WindowMove::Begins
            {
                window_move = WindowMove::Begins;
            }
        }
        if let Some(press) = press
            && window_move == WindowMove::NotBegun
        {
            let _a_press_starts_no_drag =
                (self.transfers.drags).track_gesture(surface, &press, &mut self.records);
        }
        Ok(())
    }

    /// Routes a key to the view of the focused surface. When `action` presses or repeats the key, a clipboard shortcut
    /// copies, cuts or pastes instead.
    fn key(&mut self, event: DeviceEvent, action: KeyAction) -> Result<(), ViewError> {
        let pressed = action.press_state() == PressState::Pressed;
        for routed in self.input.route(event) {
            if !pressed || self.run_clipboard_shortcut(&routed) == ShortcutOutcome::Declined {
                let _keys_start_no_move = self.deliver(routed)?;
            }
        }
        Ok(())
    }

    /// Copies, cuts or pastes text for a clipboard shortcut. The shortcut is declined when there is nothing to copy,
    /// cut or paste, when the routed surface is not open, and on a platform without a clipboard.
    fn run_clipboard_shortcut(&mut self, routed: &RoutedInput) -> ShortcutOutcome {
        let RoutedInput::Input { surface, input, .. } = routed else {
            return ShortcutOutcome::Declined;
        };
        let Some(text) = &self.transfers.text else {
            return ShortcutOutcome::Declined;
        };
        (self.records.open_view_mut(*surface)).map_or(ShortcutOutcome::Declined, |view| {
            text.apply_shortcut(*surface, input, view)
        })
    }

    /// Delivers `routed` to the view of the routed surface and sends the request that follows from it. A cancel also
    /// ends the drag gesture of the primary button of the cancelled pointer on the surface.
    fn deliver(&mut self, routed: RoutedInput) -> Result<WindowMove, ViewError> {
        if let RoutedInput::Cancel { surface, pointer } = routed {
            self.transfers.drags.cancel_gesture(pointer, surface);
        }
        let request = routed.deliver(&mut self.records);
        let window_move = match request {
            Some(PlatformRequest::WindowControl(WindowControlRequest::BeginMove { .. })) => {
                WindowMove::Begins
            }
            Some(_) | None => WindowMove::NotBegun,
        };
        if let Some(request) = request {
            self.request(request)?;
        }
        Ok(window_move)
    }

    /// Shows the physical key `code` of `keyboard` and its `action` to the key observer of the app, and returns
    /// whether the observer consumed it.
    fn host_key(&mut self, keyboard: KeyboardDevice, code: u32, action: KeyAction) -> bool {
        let Some(surface) = self.input.keyboard_focus(keyboard) else {
            return false;
        };
        let key = HostKey {
            surface,
            code,
            pressed: action.press_state() == PressState::Pressed,
            repeat: action == KeyAction::Repeated,
        };
        (self.observers.key.as_mut()).is_some_and(|observer| observer(key))
    }

    fn notify_keyboard_focus(&mut self, surface: SurfaceId, focused: bool) {
        if let Some(observer) = &mut self.observers.keyboard_focus {
            observer(KeyboardFocus { surface, focused });
        }
    }
}

/// A copy of `event` when it presses a pointer button.
fn button_press(event: &DeviceEvent) -> Option<DeviceEvent> {
    match *event {
        DeviceEvent::Button {
            pointer,
            button,
            state: PressState::Pressed,
            at,
            serial,
        } => Some(DeviceEvent::Button {
            pointer,
            button,
            state: PressState::Pressed,
            at,
            serial,
        }),
        _ => None,
    }
}
