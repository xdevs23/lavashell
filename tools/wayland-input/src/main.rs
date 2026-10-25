// SPDX-License-Identifier: GPL-3.0-only
//! Sends input to a Wayland compositor for integration tests. The program creates a virtual pointer on the first seat,
//! and with `--keyboard` a virtual keyboard as well. It reads one command per line from standard input and prints `ok`
//! after the compositor has processed the command. The virtual devices exist until standard input closes, so a test
//! leaves standard input open for as long as it sends input.
#![forbid(unsafe_code)]

mod error;
mod keymap;
#[cfg(test)]
mod test_error;

use clap::Parser;
use error::SessionError;
use keymap::Keymap;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::client::{
    Connection, EventQueue, QueueHandle, delegate_noop,
    globals::{GlobalList, registry_queue_init},
    protocol::{wl_keyboard, wl_output, wl_seat},
};
use smithay_client_toolkit::reexports::protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};
use smithay_client_toolkit::reexports::protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::{delegate_registry, registry_handlers};
use std::io::{BufRead, Write};
use std::time::Instant;
use std::{fs::OpenOptions, os::fd::AsFd, os::unix::fs::OpenOptionsExt};

#[derive(Parser)]
struct Arguments {
    #[arg(long)]
    keyboard: bool,
}

/// A command read from one line of standard input.
#[derive(Parser)]
#[command(no_binary_name = true)]
enum Command {
    Move {
        x: u32,
        y: u32,
    },
    /// Presses and releases the key with this name in the keymap, such as `SPCE`.
    Key {
        name: String,
    },
}

struct Driver {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
}

impl ProvidesRegistryState for Driver {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState, SeatState];
}

delegate_registry!(Driver);

delegate_noop!(Driver: ignore ZwlrVirtualPointerManagerV1);

delegate_noop!(Driver: ignore ZwlrVirtualPointerV1);

smithay_client_toolkit::delegate_dispatch2!(Driver);

delegate_noop!(Driver: ignore ZwpVirtualKeyboardManagerV1);

delegate_noop!(Driver: ignore ZwpVirtualKeyboardV1);

impl SeatHandler for Driver {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl OutputHandler for Driver {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl Driver {
    /// The size of the box around the logical rectangles of all outputs, which is the space that the compositor
    /// maps absolute pointer positions onto.
    fn layout_extent(&self) -> Result<(u32, u32), SessionError> {
        let mut corners: Option<((i32, i32), (i32, i32))> = None;
        for output in self.outputs.outputs() {
            let info = self
                .outputs
                .info(&output)
                .ok_or(SessionError::OutputWithoutLogicalRectangle)?;
            let ((left, top), (width, height)) = info
                .logical_position
                .zip(info.logical_size)
                .ok_or(SessionError::OutputWithoutLogicalRectangle)?;
            let (low, high) = corners.unwrap_or(((left, top), (left + width, top + height)));
            corners = Some((
                (low.0.min(left), low.1.min(top)),
                (high.0.max(left + width), high.1.max(top + height)),
            ));
        }
        let ((left, top), (right, bottom)) = corners.ok_or(SessionError::NoOutput)?;
        Ok((u32::try_from(right - left)?, u32::try_from(bottom - top)?))
    }
}

fn main() -> Result<(), SessionError> {
    let mut session = Session::open(&Arguments::parse())?;
    println!("ready {}", std::process::id());
    std::io::stdout().flush()?;
    session.run(std::io::stdin().lock())?;
    session.close()
}

/// Waits until the compositor has replied to every request sent so far.
fn synchronize(queue: &mut EventQueue<Driver>, driver: &mut Driver) -> Result<(), SessionError> {
    queue.roundtrip(driver).map(|_events| ())?;
    Ok(())
}

struct Session {
    connection: Connection,
    queue: EventQueue<Driver>,
    driver: Driver,
    pointers: ZwlrVirtualPointerManagerV1,
    pointer: ZwlrVirtualPointerV1,
    keyboard: Option<VirtualKeyboard>,
    /// The size of the area in which absolute pointer positions are given: the box around the logical rectangles of
    /// all outputs.
    extent: (u32, u32),
    start: Instant,
}

impl Session {
    fn open(arguments: &Arguments) -> Result<Self, SessionError> {
        let connection = Connection::connect_to_env()?;
        let (globals, mut queue) = registry_queue_init::<Driver>(&connection)?;
        let qh = queue.handle();
        let mut driver = Driver {
            registry: RegistryState::new(&globals),
            seats: SeatState::new(&globals, &qh),
            outputs: OutputState::new(&globals, &qh),
        };
        synchronize(&mut queue, &mut driver)?;
        let seat = driver.seats.seats().next().ok_or(SessionError::NoSeat)?;
        let extent = driver.layout_extent()?;
        let pointers: ZwlrVirtualPointerManagerV1 = globals.bind(&qh, 1..=2, ())?;
        let pointer = pointers.create_virtual_pointer(Some(&seat), &qh, ());
        let keyboard = if arguments.keyboard {
            Some(VirtualKeyboard::new(
                &globals,
                &qh,
                seat,
                Keymap::compile()?,
            )?)
        } else {
            None
        };
        synchronize(&mut queue, &mut driver)?;
        Ok(Self {
            connection,
            queue,
            driver,
            pointers,
            pointer,
            keyboard,
            extent,
            start: Instant::now(),
        })
    }

    fn run(&mut self, commands: impl BufRead) -> Result<(), SessionError> {
        for line in commands.lines() {
            self.apply(&Command::try_parse_from(line?.split_whitespace())?)?;
            self.pointer.frame();
            synchronize(&mut self.queue, &mut self.driver)?;
            println!("ok");
            std::io::stdout().flush()?;
        }
        Ok(())
    }

    /// Wayland timestamps count milliseconds and wrap around at the width of a `u32`, so a timestamp is the
    /// lowest four bytes of the elapsed milliseconds.
    fn timestamp(&self) -> u32 {
        let [lowest, second, third, fourth, ..] = self.start.elapsed().as_millis().to_le_bytes();
        u32::from_le_bytes([lowest, second, third, fourth])
    }

    fn apply(&self, command: &Command) -> Result<(), SessionError> {
        use wl_keyboard::KeyState;
        let time = self.timestamp();
        let (width, height) = self.extent;
        match *command {
            Command::Move { x, y } => self.pointer.motion_absolute(time, x, y, width, height),
            Command::Key { ref name } => {
                let keyboard = self.keyboard()?;
                keyboard.key(time, name, KeyState::Pressed)?;
                keyboard.key(time, name, KeyState::Released)?;
            }
        }
        Ok(())
    }

    fn keyboard(&self) -> Result<&VirtualKeyboard, SessionError> {
        self.keyboard.as_ref().ok_or(SessionError::KeyboardDisabled)
    }

    fn close(self) -> Result<(), SessionError> {
        self.pointer.destroy();
        if let Some(keyboard) = self.keyboard {
            keyboard.close();
        }
        self.pointers.destroy();
        self.connection.flush()?;
        Ok(())
    }
}

struct VirtualKeyboard {
    device: ZwpVirtualKeyboardV1,
    manager: ZwpVirtualKeyboardManagerV1,
    seat: wl_seat::WlSeat,
    keymap: Keymap,
}

impl VirtualKeyboard {
    fn new(
        globals: &GlobalList,
        qh: &QueueHandle<Driver>,
        seat: wl_seat::WlSeat,
        keymap: Keymap,
    ) -> Result<Self, SessionError> {
        let manager: ZwpVirtualKeyboardManagerV1 = globals.bind(qh, 1..=1, ())?;
        let device = manager.create_virtual_keyboard(&seat, qh, ());
        let path = std::path::PathBuf::from(
            std::env::var_os("XDG_RUNTIME_DIR").ok_or(SessionError::RuntimeDirectoryMissing)?,
        )
        .join(format!("lavashell-input-keymap-{}", std::process::id()));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        let map = keymap.text();
        file.write_all(map.as_bytes())?;
        file.write_all(&[0])?;
        let format = wl_keyboard::KeymapFormat::XkbV1.into();
        device.keymap(format, file.as_fd(), u32::try_from(map.len() + 1)?);
        std::fs::remove_file(path)?;
        Ok(Self {
            device,
            manager,
            seat,
            keymap,
        })
    }

    fn key(&self, time: u32, name: &str, state: wl_keyboard::KeyState) -> Result<(), SessionError> {
        self.device.key(time, self.keymap.code(name)?, state.into());
        Ok(())
    }

    fn close(self) {
        self.device.destroy();
        drop(self.manager);
        self.seat.release();
    }
}

#[cfg(test)]
mod tests;
