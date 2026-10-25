// SPDX-License-Identifier: GPL-3.0-only
use super::preferences::HostRepeat;
use super::{
    Capability, Connection, Cursor, CursorIcon, Duration, InputEvent, InputModifiers, Key,
    KeyEvent, KeyboardHandler, Keysym, Modifiers, Platform, PointerEvent, PointerEventKind,
    PointerHandler, QueueHandle, RawModifiers, SeatDevices, SeatHandler, SeatState, ThemeSpec,
    sctk, wl_keyboard, wl_pointer, wl_seat, wl_surface,
};
use lavashell_core::platform::KeyRepeat;
use sctk::seat::keyboard::RepeatInfo;
use sctk::seat::pointer::{BTN_LEFT, BTN_RIGHT};

/// A physical key of the focused surface, as a Linux evdev code independent of the keyboard
/// layout and of text editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostKey {
    pub surface: usize,
    pub code: u32,
    pub pressed: bool,
    pub repeat: bool,
}

/// A surface that gained or lost the keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardFocus {
    pub surface: usize,
    pub focused: bool,
}

impl Platform {
    pub(super) fn keyboard_seat(&self, keyboard: &wl_keyboard::WlKeyboard) -> Option<&SeatDevices> {
        (self.devices.iter()).find(|seat| seat.keyboard.as_ref() == Some(keyboard))
    }

    fn keyboard_seat_mut(
        &mut self,
        keyboard: &wl_keyboard::WlKeyboard,
    ) -> Option<&mut SeatDevices> {
        (self.devices.iter_mut()).find(|seat| seat.keyboard.as_ref() == Some(keyboard))
    }

    fn cancel_pointer(&mut self, seat: &wl_seat::WlSeat) {
        let pointer = self
            .devices
            .iter()
            .find(|device| &device.seat == seat)
            .and_then(|device| device.pointer.as_ref())
            .map(smithay_client_toolkit::seat::pointer::ThemedPointer::pointer);
        if let Some(pointer) = pointer {
            for surface in &mut self.surfaces {
                if surface.pointer_owner.as_ref() == Some(pointer) {
                    surface.pointer_owner = None;
                    surface.view.input(InputEvent::CancelPointer);
                }
            }
        }
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
        let Some(surface) = self.keyboard_seat(keyboard).and_then(|seat| seat.focus) else {
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
        if matches!(capability, Capability::Pointer) {
            self.cancel_pointer(&seat);
            self.transfer_pointer_cancel(&seat);
        }
        if matches!(capability, Capability::Touch) {
            self.cancel_touches(&seat);
        }
        if let Some(device) = self.devices.iter_mut().find(|device| device.seat == seat) {
            match capability {
                Capability::Pointer => device.pointer = None,
                Capability::Keyboard => {
                    if let Some(keyboard) = device.keyboard.take() {
                        keyboard.release();
                    }
                    device.focus = None;
                }
                Capability::Touch => {
                    if let Some(touch) = device.touch.take() {
                        touch.release();
                    }
                }
                _ => {}
            }
        }
        self.accessibility_focus();
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.cancel_pointer(&seat);
        self.cancel_touches(&seat);
        self.remove_transfer_seat(&seat);
        self.devices.retain(|device| device.seat != seat);
        self.accessibility_focus();
    }
}

impl PointerHandler for Platform {
    fn pointer_frame(
        &mut self,
        connection: &Connection,
        _: &QueueHandle<Self>,
        pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let Some(index) = self.index(&event.surface) else {
                continue;
            };
            if let PointerEventKind::Release { button, .. } = event.kind {
                self.transfer_pointer_release(pointer, &event.surface, button);
            }
            if matches!(
                event.kind,
                PointerEventKind::Leave { .. } | PointerEventKind::Release { .. }
            ) && self.surfaces[index].pointer_owner.as_ref() != Some(pointer)
            {
                continue;
            }
            if let Some(previous) = self.surfaces[index].pointer_owner.replace(pointer.clone())
                && previous != *pointer
            {
                self.surfaces[index].view.input(InputEvent::CancelPointer);
                self.transfer_pointer_release(&previous, &event.surface, BTN_LEFT);
            }
            let at = crate::logical::point(event.position.0, event.position.1);
            match event.kind {
                PointerEventKind::Press {
                    serial,
                    button: BTN_LEFT,
                    ..
                } if self.move_window(index, pointer, at, serial) => {
                    continue;
                }
                PointerEventKind::Press { serial, button, .. } => {
                    self.transfer_pointer_press(pointer, &event.surface, serial, button, at);
                }
                PointerEventKind::Motion { .. } if self.transfer_pointer_motion(pointer, at) => {
                    continue;
                }
                _ => {}
            }
            if let PointerEventKind::Press { serial, .. } | PointerEventKind::Release { serial, .. } =
                event.kind
                && let Some(seat) = self.devices.iter().find(|seat| {
                    (seat.pointer.as_ref()).is_some_and(|themed| themed.pointer() == pointer)
                })
            {
                self.clipboard.note_serial(seat.seat.clone(), serial);
            }
            let input = match &event.kind {
                PointerEventKind::Leave { .. } => Some(InputEvent::Leave),
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    Some(InputEvent::Move(at))
                }
                PointerEventKind::Press { button, .. } if *button == BTN_LEFT => {
                    Some(InputEvent::Press(at))
                }
                PointerEventKind::Press { button, .. } if *button == BTN_RIGHT => {
                    Some(InputEvent::Context(at))
                }
                PointerEventKind::Release { button, .. } if *button == BTN_LEFT => {
                    Some(InputEvent::Release(at))
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => Some(InputEvent::Scroll {
                    at,
                    delta: crate::logical::point(horizontal.absolute, vertical.absolute),
                }),
                _ => None,
            };
            if let Some(input) = input {
                self.surfaces[index].view.input(input);
                if let Some(themed) = self
                    .devices
                    .iter()
                    .filter_map(|seat| seat.pointer.as_ref())
                    .find(|themed| themed.pointer() == pointer)
                {
                    let icon = match self.surfaces[index].view.cursor() {
                        Cursor::Default => CursorIcon::Default,
                        Cursor::Pointer => CursorIcon::Pointer,
                        Cursor::Text => CursorIcon::Text,
                    };
                    if let Err(error) = themed.set_cursor(connection, icon) {
                        log::warn!("the pointer keeps its cursor: {error}");
                    }
                }
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
            )
            .map_or(HostRepeat::Disabled, HostRepeat::Repeats),
            RepeatInfo::Disable => HostRepeat::Disabled,
        };
        let views = (self.surfaces.iter())
            .filter(|surface| surface.lifecycle.is_open())
            .map(|surface| (&surface.view, &surface.supplied_facts));
        self.view_configuration.repeat(repeat, views);
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
        let focus = self.index(wl);
        if let Some(surface) = focus
            && let Some(observer) = &mut self.config.focus_observer
        {
            observer(KeyboardFocus {
                surface,
                focused: true,
            });
        }
        if let Some(seat) = self.keyboard_seat_mut(keyboard) {
            seat.focus = focus;
        }
        self.accessibility_focus();
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        let left = self
            .keyboard_seat_mut(keyboard)
            .and_then(|seat| seat.focus.take());
        if let Some(surface) = left
            && let Some(observer) = &mut self.config.focus_observer
        {
            observer(KeyboardFocus {
                surface,
                focused: false,
            });
        }
        self.accessibility_focus();
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
            self.clipboard.note_serial(seat.seat.clone(), serial);
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
        if let Some(index) = self.keyboard_seat(keyboard).and_then(|seat| seat.focus) {
            let key = match event.keysym {
                Keysym::Return | Keysym::KP_Enter => Some(Key::Enter),
                Keysym::space => Some(Key::Space),
                _ => None,
            };
            if let Some(key) = key {
                self.surfaces[index].view.input(InputEvent::KeyRelease(key));
            }
        }
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
        if let Some(seat) = self.keyboard_seat_mut(keyboard) {
            seat.modifiers = modifiers;
            if let Some(index) = seat.focus {
                self.surfaces[index]
                    .view
                    .input(InputEvent::Modifiers(InputModifiers {
                        control: modifiers.ctrl,
                        shift: modifiers.shift,
                        alt: modifiers.alt,
                    }));
            }
        }
    }
}
