// SPDX-License-Identifier: GPL-3.0-only
use super::device::{keyboard_device, pointer_device, pointer_event};
use super::{SeatDevices, key};
use crate::translator::Translator;
use lavashell_core::ui::InputModifiers;
use lavashell_core::view::Cursor;
use lavashell_platform::{
    Button, DeviceEvent, InputRequest, KeyAction, KeyboardDevice, PointerDevice, PressState,
    SettingsEvent,
};
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

impl SeatDevices {
    fn pointer_device(&self) -> Option<PointerDevice> {
        (self.pointer.as_ref()).map(|themed| pointer_device(themed.pointer()))
    }

    fn keyboard_device(&self) -> Option<KeyboardDevice> {
        self.keyboard.as_ref().map(keyboard_device)
    }
}

impl Translator {
    pub(super) fn keyboard_seat(&self, keyboard: &wl_keyboard::WlKeyboard) -> Option<&SeatDevices> {
        (self.devices.iter()).find(|seat| seat.keyboard.as_ref() == Some(keyboard))
    }

    pub(super) fn seat_devices(&mut self, seat: &wl_seat::WlSeat) -> Option<&mut SeatDevices> {
        (self.devices.iter_mut()).find(|known| &known.seat == seat)
    }

    /// Executes `request` while the pointer named in it exists.
    pub(crate) fn execute_input(&self, request: InputRequest) {
        match request {
            InputRequest::SetCursor(pointer, cursor) => {
                if let Some(themed) = self.themed_pointer(pointer)
                    && let Err(error) = themed.set_cursor(&self.connection, cursor_icon(cursor))
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
}

fn cursor_icon(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Text => CursorIcon::Text,
    }
}

impl SeatHandler for Translator {
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
                            state.report_key(keyboard, event, KeyAction::Repeated);
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
                    let pointer = pointer_device(themed.pointer());
                    self.links.report(DeviceEvent::PointerRemoved(pointer));
                }
                self.cancel_drag_sources(&seat);
            }
            Capability::Keyboard => {
                let removed = self
                    .seat_devices(&seat)
                    .and_then(|known| known.keyboard.take());
                if let Some(keyboard) = removed {
                    self.links
                        .report(DeviceEvent::KeyboardRemoved(keyboard_device(&keyboard)));
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
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.cancel_touches(&seat);
        self.remove_transfer_seat(&seat);
        if let Some(index) = self.devices.iter().position(|known| known.seat == seat) {
            let removed = self.devices.remove(index);
            let pointer = removed.pointer_device().map(DeviceEvent::PointerRemoved);
            let keyboard = removed.keyboard_device().map(DeviceEvent::KeyboardRemoved);
            for removal in pointer.into_iter().chain(keyboard) {
                self.links.report(removal);
            }
        }
    }
}

impl PointerHandler for Translator {
    /// Reports each event of the frame on a known surface. The serial of a press or a release is noted for the
    /// clipboard, and the serial of a press of the primary button for a drag started by the app.
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
            let at = crate::logical::point(event.position.0, event.position.1);
            let input = pointer_event(device, id, at, &event.kind);
            let primary_press = is_primary_press(&input);
            self.links.report(input);
            if let PointerEventKind::Press { serial, .. }
            | PointerEventKind::Release { serial, .. } = event.kind
            {
                if primary_press {
                    self.note_primary_press(device, serial);
                }
                if let Some(seat) = self.pointer_seat(device).cloned() {
                    self.note_input_serial(seat, serial);
                }
            }
        }
    }
}

impl KeyboardHandler for Translator {
    fn update_repeat_info(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        info: RepeatInfo,
    ) {
        let repeat = key::key_repeat(info);
        self.links.report(SettingsEvent::KeyRepeat(repeat));
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
        self.links.report(DeviceEvent::KeyboardFocus {
            keyboard: keyboard_device(keyboard),
            surface: self.find_surface(wl),
        });
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        self.links.report(DeviceEvent::KeyboardFocus {
            keyboard: keyboard_device(keyboard),
            surface: None,
        });
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
        self.report_key(keyboard, event, KeyAction::Pressed);
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.report_key(keyboard, event, KeyAction::Repeated);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.report_key(keyboard, event, KeyAction::Released);
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
        self.links.report(DeviceEvent::Modifiers {
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
