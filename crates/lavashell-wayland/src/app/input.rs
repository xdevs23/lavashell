// SPDX-License-Identifier: GPL-3.0-only
use super::device::{keyboard_device, pointer_device, pointer_event};
use super::{Platform, SeatDevices, key};
use crate::Error;
use lavashell_core::platform::KeyRepeat;
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::InputModifiers;
use lavashell_core::view::Cursor;
use lavashell_platform::SettingsEvent;
use lavashell_platform::{
    Button, DeviceEvent, InputRequest, KeyboardDevice, PlatformRequest, PointerDevice, PressState,
};
use lavashell_runtime::RoutedInput;
use sctk::reexports::client::protocol::{wl_keyboard, wl_pointer, wl_seat, wl_surface};
use sctk::reexports::client::{Connection, QueueHandle};
use sctk::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers, RepeatInfo,
};
use sctk::seat::pointer::{
    CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer,
};
use sctk::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit as sctk;
use std::time::Duration;

/// A physical key of the focused surface, as a Linux evdev code independent of the keyboard
/// layout and of text editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostKey {
    pub surface: SurfaceId,
    pub code: u32,
    pub pressed: bool,
    pub repeat: bool,
}

/// A surface that gained or lost the keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardFocus {
    pub surface: SurfaceId,
    pub focused: bool,
}

impl SeatDevices {
    fn pointer_device(&self) -> Option<PointerDevice> {
        (self.pointer.as_ref()).map(|themed| pointer_device(themed.pointer()))
    }

    fn keyboard_device(&self) -> Option<KeyboardDevice> {
        self.keyboard.as_ref().map(keyboard_device)
    }
}

impl Platform {
    pub(super) fn keyboard_seat(&self, keyboard: &wl_keyboard::WlKeyboard) -> Option<&SeatDevices> {
        (self.devices.iter()).find(|seat| seat.keyboard.as_ref() == Some(keyboard))
    }

    pub(super) fn seat_devices(&mut self, seat: &wl_seat::WlSeat) -> Option<&mut SeatDevices> {
        (self.devices.iter_mut()).find(|known| &known.seat == seat)
    }

    /// Hands `event` to the runtime and delivers each input that follows from it.
    pub(super) fn route_input(&mut self, event: DeviceEvent) {
        for routed in self.input.route(event) {
            let _moves_start_only_in_pointer_frames = self.deliver(routed);
        }
    }

    /// Delivers `routed` to the view of the routed surface and executes the request that follows from it. A cancel
    /// also ends the drag gesture of the primary button of the cancelled pointer on the surface. Returns whether a
    /// move of the window started.
    pub(super) fn deliver(&mut self, routed: RoutedInput) -> bool {
        if let RoutedInput::Cancel { surface, pointer } = routed {
            self.config.drags.cancel_gesture(pointer, surface);
        }
        let Some(request) = routed.deliver(&mut self.surface_records) else {
            return false;
        };
        match self.execute_request(request) {
            Ok(moving) => moving,
            Err(error) => {
                self.input_failure = Some(error);
                false
            }
        }
    }

    /// Executes `request` and returns whether a move of the window started.
    fn execute_request(&self, request: PlatformRequest) -> Result<bool, Error> {
        match request {
            PlatformRequest::Input(request) => {
                self.execute_input_request(request);
                Ok(false)
            }
            PlatformRequest::WindowControl(request) => self.execute_window_control(request),
        }
    }

    fn execute_input_request(&self, request: InputRequest) {
        match request {
            InputRequest::SetCursor(pointer, cursor) => {
                let icon = match cursor {
                    Cursor::Default => CursorIcon::Default,
                    Cursor::Pointer => CursorIcon::Pointer,
                    Cursor::Text => CursorIcon::Text,
                };
                if let Some(themed) = self.themed_pointer(pointer)
                    && let Err(error) = themed.set_cursor(&self.connection, icon)
                {
                    log::warn!("cannot set the cursor of the pointer: {error}");
                }
            }
        }
    }

    pub(crate) fn pointer_seat(&self, pointer: PointerDevice) -> Option<&wl_seat::WlSeat> {
        (self.devices.iter())
            .find(|seat| seat.pointer_device() == Some(pointer))
            .map(|seat| &seat.seat)
    }

    fn themed_pointer(&self, pointer: PointerDevice) -> Option<&ThemedPointer> {
        (self.devices.iter())
            .filter_map(|seat| seat.pointer.as_ref())
            .find(|themed| pointer_device(themed.pointer()) == pointer)
    }

    fn dispatch_key(&mut self, keyboard: &wl_keyboard::WlKeyboard, event: KeyEvent, repeat: bool) {
        if !self.host_key(keyboard, &event, true, repeat) {
            self.key(keyboard, event);
        }
    }

    fn host_key(
        &mut self,
        keyboard: &wl_keyboard::WlKeyboard,
        event: &KeyEvent,
        pressed: bool,
        repeat: bool,
    ) -> bool {
        let Some(surface) = self.input.keyboard_focus(keyboard_device(keyboard)) else {
            return false;
        };
        self.config.key_observer.as_mut().is_some_and(|callback| {
            callback(HostKey {
                surface,
                code: event.raw_code,
                pressed,
                repeat,
            })
        })
    }
}

impl SeatHandler for Platform {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        if !self.devices.iter().any(|device| device.seat == seat) {
            self.clipboard.add_seat(&self.qh, seat.clone());
            self.devices.push(SeatDevices::new(seat));
        }
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        let Some(index) = self.devices.iter().position(|device| device.seat == seat) else {
            return;
        };
        match capability {
            Capability::Keyboard => {
                self.devices[index].keyboard = self
                    .seats
                    .get_keyboard_with_repeat(
                        qh,
                        &seat,
                        None,
                        self.loop_handle.clone(),
                        Box::new(|state, keyboard, event| {
                            state.dispatch_key(keyboard, event, true);
                        }),
                    )
                    .inspect_err(|error| log::warn!("the keyboard of a seat is not used: {error}"))
                    .ok();
            }
            Capability::Pointer => {
                self.devices[index].pointer = self
                    .seats
                    .get_pointer_with_theme::<Self, ()>(
                        qh,
                        &seat,
                        self.shm.wl_shm(),
                        self.compositor.create_surface(qh),
                        ThemeSpec::default(),
                    )
                    .inspect_err(|error| log::warn!("the pointer of a seat is not used: {error}"))
                    .ok();
            }
            Capability::Touch => {
                self.devices[index].touch = (self.seats.get_touch(qh, &seat))
                    .inspect_err(|error| {
                        log::warn!("the touch screen of a seat is not used: {error}");
                    })
                    .ok();
            }
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Pointer => {
                let removed = self
                    .seat_devices(&seat)
                    .and_then(|known| known.pointer.take());
                if let Some(themed) = removed {
                    self.route_input(DeviceEvent::PointerRemoved(pointer_device(
                        themed.pointer(),
                    )));
                }
                self.cancel_drag_sources(&seat);
            }
            Capability::Keyboard => {
                let removed = self
                    .seat_devices(&seat)
                    .and_then(|known| known.keyboard.take());
                if let Some(keyboard) = removed {
                    self.route_input(DeviceEvent::KeyboardRemoved(keyboard_device(&keyboard)));
                    keyboard.release();
                }
            }
            Capability::Touch => {
                self.cancel_touches(&seat);
                if let Some(touch) = self
                    .seat_devices(&seat)
                    .and_then(|known| known.touch.take())
                {
                    touch.release();
                }
            }
            _ => {}
        }
        self.update_window_focus();
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.cancel_touches(&seat);
        self.remove_transfer_seat(&seat);
        if let Some(index) = self.devices.iter().position(|known| known.seat == seat) {
            let removed = self.devices.remove(index);
            let pointer = removed.pointer_device().map(DeviceEvent::PointerRemoved);
            let keyboard = removed.keyboard_device().map(DeviceEvent::KeyboardRemoved);
            for removal in pointer.into_iter().chain(keyboard) {
                self.route_input(removal);
            }
        }
        self.update_window_focus();
    }
}

impl PointerHandler for Platform {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let device = pointer_device(pointer);
        for event in events {
            let Some(id) = self.find_surface(&event.surface) else {
                continue;
            };
            let interacting = self.input.interacting_pointer(id) == Some(device);
            let at = crate::logical::point(event.position.0, event.position.1);
            let input = pointer_event(device, id, at, &event.kind);
            let dragging = !matches!(event.kind, PointerEventKind::Press { .. })
                && (self.config.drags).track_gesture(id, &input, &mut self.surface_records);
            let routed = self.input.route(input);
            let mut moving = false;
            for routed in routed {
                if !dragging || matches!(routed, RoutedInput::Cancel { .. }) {
                    moving |= self.deliver(routed);
                }
            }
            let serial = match event.kind {
                PointerEventKind::Press { serial, .. } if !moving => {
                    let press = pointer_event(device, id, at, &event.kind);
                    if is_primary_press(&press) {
                        self.note_primary_press(device, serial);
                    }
                    let _a_press_starts_no_drag =
                        (self.config.drags).track_gesture(id, &press, &mut self.surface_records);
                    Some(serial)
                }
                PointerEventKind::Release { serial, .. } if interacting => Some(serial),
                _ => None,
            };
            if let Some(serial) = serial
                && let Some(seat) = self.pointer_seat(device).cloned()
            {
                self.note_input_serial(seat, serial);
            }
        }
    }
}

impl KeyboardHandler for Platform {
    fn update_repeat_info(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        info: RepeatInfo,
    ) {
        let repeat = match info {
            RepeatInfo::Repeat { rate, delay } => KeyRepeat::new(
                Duration::from_millis(delay.into()),
                Duration::from_secs(1) / rate.get(),
            ),
            RepeatInfo::Disable => None,
        };
        (self.surface_records).change_settings(SettingsEvent::KeyRepeat(repeat));
    }

    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        wl: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        let focus = self.find_surface(wl);
        if let Some(surface) = focus
            && let Some(observer) = &mut self.config.focus_observer
        {
            observer(KeyboardFocus {
                surface,
                focused: true,
            });
        }
        self.route_input(DeviceEvent::KeyboardFocus {
            keyboard: keyboard_device(keyboard),
            surface: focus,
        });
        self.update_window_focus();
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        let device = keyboard_device(keyboard);
        let left = self.input.keyboard_focus(device);
        self.route_input(DeviceEvent::KeyboardFocus {
            keyboard: device,
            surface: None,
        });
        if let Some(surface) = left
            && let Some(observer) = &mut self.config.focus_observer
        {
            observer(KeyboardFocus {
                surface,
                focused: false,
            });
        }
        self.update_window_focus();
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        if let Some(seat) = self.keyboard_seat(keyboard) {
            let seat = seat.seat.clone();
            self.note_input_serial(seat, serial);
        }
        self.dispatch_key(keyboard, event, false);
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.dispatch_key(keyboard, event, true);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if self.host_key(keyboard, &event, false, false) {
            return;
        }
        self.route_input(DeviceEvent::Key {
            keyboard: keyboard_device(keyboard),
            stroke: key::stroke(event),
            state: PressState::Released,
        });
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.route_input(DeviceEvent::Modifiers {
            keyboard: keyboard_device(keyboard),
            modifiers: InputModifiers {
                control: modifiers.ctrl,
                shift: modifiers.shift,
                alt: modifiers.alt,
            },
        });
    }
}

fn is_primary_press(event: &DeviceEvent) -> bool {
    matches!(
        event,
        DeviceEvent::Button {
            button: Button::Primary,
            state: PressState::Pressed,
            ..
        }
    )
}
