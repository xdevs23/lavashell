// SPDX-License-Identifier: GPL-3.0-only
use super::ViewActor;
use crate::clipboard::ShortcutOutcome;
use crate::input::RoutedInput;
use crate::observers::{HostKey, KeyboardFocus};
use crate::rendered_platform::RenderedPlatform;
use crate::view_actor::ViewError;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{DeviceEvent, KeyAction, KeyboardDevice, PlatformRequest, PressState};

#[derive(Clone, Copy, PartialEq, Eq)]
enum InteractiveWindowControl {
    Begins,
    NotBegun,
}

impl<P: RenderedPlatform> ViewActor<P> {
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
            let _window_controls_begin_only_from_pointer_input = self.deliver(routed)?;
        }
        Ok(())
    }

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
        let records = self.surfaces.records_mut();
        let dragging = press.is_none() && drags.track_gesture(surface, &event, records);
        let mut interactive_control = InteractiveWindowControl::NotBegun;
        for routed in self.input.route(event) {
            if (!dragging || matches!(routed, RoutedInput::Cancel { .. }))
                && self.deliver(routed)? == InteractiveWindowControl::Begins
            {
                interactive_control = InteractiveWindowControl::Begins;
            }
        }
        if let Some(press) = press
            && interactive_control == InteractiveWindowControl::NotBegun
        {
            let _a_press_starts_no_drag =
                (self.transfers.drags).track_gesture(surface, &press, self.surfaces.records_mut());
        }
        Ok(())
    }

    fn key(&mut self, event: DeviceEvent, action: KeyAction) -> Result<(), ViewError> {
        let pressed = action.press_state() == PressState::Pressed;
        for routed in self.input.route(event) {
            if !pressed || self.run_clipboard_shortcut(&routed) == ShortcutOutcome::Declined {
                let _keys_begin_no_window_control = self.deliver(routed)?;
            }
        }
        Ok(())
    }

    fn run_clipboard_shortcut(&mut self, routed: &RoutedInput) -> ShortcutOutcome {
        let RoutedInput::Input { surface, input, .. } = routed else {
            return ShortcutOutcome::Declined;
        };
        let Some(text) = &self.transfers.text else {
            return ShortcutOutcome::Declined;
        };
        (self.surfaces.records_mut().open_view_mut(*surface))
            .map_or(ShortcutOutcome::Declined, |view| {
                text.apply_shortcut(*surface, input, view)
            })
    }

    fn deliver(&mut self, routed: RoutedInput) -> Result<InteractiveWindowControl, ViewError> {
        if let RoutedInput::Cancel { surface, pointer } = routed {
            self.transfers.drags.cancel_gesture(pointer, surface);
        }
        let request = routed.deliver(self.surfaces.records_mut());
        let interactive_control = match request.as_ref() {
            Some(PlatformRequest::WindowControl(control)) if control.begins_pointer_grab() => {
                InteractiveWindowControl::Begins
            }
            Some(_) | None => InteractiveWindowControl::NotBegun,
        };
        if let Some(request) = request {
            self.request(request)?;
        }
        Ok(interactive_control)
    }

    fn host_key(&mut self, keyboard: KeyboardDevice, code: u32, action: KeyAction) -> bool {
        let Some(surface) = self.input.keyboard_focus(keyboard) else {
            return false;
        };
        let key = HostKey {
            surface,
            code,
            action,
        };
        (self.observers.key.as_mut()).is_some_and(|observer| observer(key))
    }

    fn notify_keyboard_focus(&mut self, surface: SurfaceId, focused: bool) {
        if let Some(observer) = &mut self.observers.keyboard_focus {
            observer(KeyboardFocus { surface, focused });
        }
    }
}

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
