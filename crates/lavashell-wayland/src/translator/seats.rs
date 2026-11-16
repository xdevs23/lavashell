// SPDX-License-Identifier: GPL-3.0-only
use super::Translator;
use crate::app::{SeatDevice, cursor_icon, is_primary_press, key_event, key_repeat};
use crate::app::{keyboard_device, pointer_device, pointer_event};
use crate::protocol_serial::ProtocolSerial;
use crate::sctk;
use lavashell_core::ui::InputModifiers;
use lavashell_platform::{DeviceEvent, InputRequest, KeyAction, SettingsEvent};
use sctk::reexports::client::protocol::{wl_keyboard, wl_pointer, wl_seat, wl_surface};
use sctk::reexports::client::{Connection, QueueHandle};
use sctk::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers, RepeatInfo,
};
use sctk::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler, ThemeSpec};
use sctk::seat::{Capability, SeatHandler, SeatState};

impl Translator {
    pub(super) fn execute_input(&self, request: InputRequest) {
        match request {
            InputRequest::SetCursor(pointer, cursor) => {
                if let Some(themed) = self.devices.themed_pointer(pointer)
                    && let Err(error) = themed.set_cursor(&self.connection, cursor_icon(cursor))
                {
                    log::warn!("cannot set the cursor of the pointer: {error}");
                }
            }
        }
    }

    fn report_key(&self, keyboard: &wl_keyboard::WlKeyboard, event: KeyEvent, action: KeyAction) {
        self.links
            .report(key_event(keyboard_device(keyboard), event, action));
    }

    fn new_device(
        &mut self,
        qh: &QueueHandle<Self>,
        seat: &wl_seat::WlSeat,
        capability: Capability,
    ) -> Option<SeatDevice> {
        match capability {
            Capability::Keyboard => self
                .seats
                .get_keyboard_with_repeat(
                    qh,
                    seat,
                    None,
                    self.loop_handle.clone(),
                    Box::new(|state, keyboard, event| {
                        state.report_key(keyboard, event, KeyAction::Repeated);
                    }),
                )
                .inspect_err(|error| log::warn!("the keyboard of a seat is not used: {error}"))
                .ok()
                .map(SeatDevice::Keyboard),
            Capability::Pointer => self
                .seats
                .get_pointer_with_theme::<Self, ()>(
                    qh,
                    seat,
                    self.shm.wl_shm(),
                    self.compositor.create_surface(qh),
                    ThemeSpec::default(),
                )
                .inspect_err(|error| log::warn!("the pointer of a seat is not used: {error}"))
                .ok()
                .map(SeatDevice::Pointer),
            Capability::Touch => (self.seats.get_touch(qh, seat))
                .inspect_err(|error| log::warn!("the touch screen of a seat is not used: {error}"))
                .ok()
                .map(SeatDevice::Touch),
            _ => None,
        }
    }
}

impl SeatHandler for Translator {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        if self.devices.insert(seat.clone()) {
            self.clipboard.add_seat(&self.qh, seat);
        }
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if self.devices.contains(&seat)
            && let Some(device) = self.new_device(qh, &seat, capability)
        {
            self.devices.attach(&seat, device);
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
                if let Some(removed) = self.devices.detach_pointer(&seat) {
                    self.links.report(removed);
                }
                self.cancel_drag_sources(&seat);
            }
            Capability::Keyboard => {
                if let Some(removed) = self.devices.detach_keyboard(&seat) {
                    self.links.report(removed);
                }
            }
            Capability::Touch => {
                for cancelled in self.devices.detach_touch(&seat) {
                    self.links.report(cancelled);
                }
            }
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        for cancelled in self.devices.cancel_seat_touches(&seat) {
            self.links.report(cancelled);
        }
        self.remove_transfer_seat(&seat);
        for removed in self.devices.remove(&seat) {
            self.links.report(removed);
        }
    }
}

impl PointerHandler for Translator {
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
            let at = crate::logical::convert_to_point(event.position.0, event.position.1);
            let input = pointer_event(device, id, at, &event.kind);
            let primary_press = is_primary_press(&input);
            self.links.report(input);
            if let PointerEventKind::Press { serial, .. } | PointerEventKind::Release { serial, .. } =
                event.kind
                && let Some(seat) = self.devices.pointer_seat(device).cloned()
            {
                let serial = ProtocolSerial::new(serial);
                if primary_press {
                    self.clipboard.note_primary_press(&seat, serial);
                }
                self.note_input_serial(seat, serial);
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
        self.links
            .report(SettingsEvent::KeyRepeat(key_repeat(info)));
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
        if let Some(seat) = self.devices.keyboard_seat(keyboard).cloned() {
            self.note_input_serial(seat, ProtocolSerial::new(serial));
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
