// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod error;
mod keymap;
mod layout;
mod next_steps;

use clap::Parser;
use error::SessionError;
use keymap::Keymap;
use lavashell_core::error::message_chain;
use lavashell_wayland_input::protocol::{Reply, Request};
use layout::{Extent, LogicalRectangle};
use next_steps::next_step;
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
use std::ops::RangeInclusive;
use std::os::fd::AsFd;
use std::process::ExitCode;
use std::time::Instant;

// https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-virtual-pointer-unstable-v1.xml
const VIRTUAL_POINTER_MANAGER_VERSIONS: RangeInclusive<u32> = 1..=2;
// https://github.com/Smithay/wayland-rs/blob/master/wayland-protocols-misc/protocols/virtual-keyboard-unstable-v1.xml
const VIRTUAL_KEYBOARD_MANAGER_VERSIONS: RangeInclusive<u32> = 1..=1;

#[derive(Parser)]
struct Arguments {
    #[arg(long)]
    keyboard: bool,
}

struct ProtocolState {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
}

impl ProvidesRegistryState for ProtocolState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState, SeatState];
}

delegate_registry!(ProtocolState);

delegate_noop!(ProtocolState: ignore ZwlrVirtualPointerManagerV1);

delegate_noop!(ProtocolState: ignore ZwlrVirtualPointerV1);

smithay_client_toolkit::delegate_dispatch2!(ProtocolState);

delegate_noop!(ProtocolState: ignore ZwpVirtualKeyboardManagerV1);

delegate_noop!(ProtocolState: ignore ZwpVirtualKeyboardV1);

impl SeatHandler for ProtocolState {
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

impl OutputHandler for ProtocolState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ProtocolState {
    fn only_seat(&self) -> Result<wl_seat::WlSeat, SessionError> {
        let mut seats = self.seats.seats();
        match (seats.next(), seats.next()) {
            (Some(seat), None) => Ok(seat),
            (None, _) => Err(SessionError::NoSeat),
            (Some(_), Some(_)) => Err(SessionError::AmbiguousSeat),
        }
    }

    fn layout_extent(&self) -> Result<Extent, SessionError> {
        let mut layout: Option<LogicalRectangle> = None;
        for output in self.outputs.outputs() {
            let rectangle = self.logical_rectangle(&output)?;
            layout = Some(layout.map_or(rectangle, |layout| layout.union(rectangle)));
        }
        layout.ok_or(SessionError::NoOutput)?.extent()
    }

    fn logical_rectangle(
        &self,
        output: &wl_output::WlOutput,
    ) -> Result<LogicalRectangle, SessionError> {
        let info = self
            .outputs
            .info(output)
            .ok_or(SessionError::OutputWithoutLogicalRectangle)?;
        let ((left, top), (width, height)) = info
            .logical_position
            .zip(info.logical_size)
            .ok_or(SessionError::OutputWithoutLogicalRectangle)?;
        Ok(LogicalRectangle {
            left,
            top,
            right: left + width,
            bottom: top + height,
        })
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}\n{}", message_chain(&error), next_step(&error));
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), SessionError> {
    let mut session = Session::open(&Arguments::parse())?;
    send_reply(&Reply::Ready {
        pid: std::process::id(),
    })?;
    session.run(std::io::stdin().lock())?;
    session.close()
}

fn send_reply(reply: &Reply) -> Result<(), SessionError> {
    let line = serde_json::to_string(reply).map_err(SessionError::EncodeReply)?;
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{line}")
        .and_then(|()| stdout.flush())
        .map_err(SessionError::SendReply)
}

fn synchronize(
    queue: &mut EventQueue<ProtocolState>,
    state: &mut ProtocolState,
) -> Result<(), SessionError> {
    let _events_already_applied_to_the_state = queue.roundtrip(state)?;
    Ok(())
}

struct Session {
    connection: Connection,
    queue: EventQueue<ProtocolState>,
    state: ProtocolState,
    pointers: ZwlrVirtualPointerManagerV1,
    pointer: ZwlrVirtualPointerV1,
    keyboard: Option<VirtualKeyboard>,
    extent: Extent,
    start: Instant,
}

impl Session {
    fn open(arguments: &Arguments) -> Result<Self, SessionError> {
        let connection = Connection::connect_to_env()?;
        let (globals, mut queue) = registry_queue_init::<ProtocolState>(&connection)?;
        let qh = queue.handle();
        let mut state = ProtocolState {
            registry: RegistryState::new(&globals),
            seats: SeatState::new(&globals, &qh),
            outputs: OutputState::new(&globals, &qh),
        };
        synchronize(&mut queue, &mut state)?;
        let seat = state.only_seat()?;
        let extent = state.layout_extent()?;
        let pointers: ZwlrVirtualPointerManagerV1 =
            globals.bind(&qh, VIRTUAL_POINTER_MANAGER_VERSIONS, ())?;
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
        synchronize(&mut queue, &mut state)?;
        Ok(Self {
            connection,
            queue,
            state,
            pointers,
            pointer,
            keyboard,
            extent,
            start: Instant::now(),
        })
    }

    fn run(&mut self, requests: impl BufRead) -> Result<(), SessionError> {
        for line in requests.lines() {
            let line = line.map_err(SessionError::ReadRequest)?;
            if let Err(failure) = self.serve(&line) {
                send_reply(&Reply::Failed {
                    message: message_chain(&failure),
                })?;
                return Err(failure);
            }
            send_reply(&Reply::Done)?;
        }
        Ok(())
    }

    fn serve(&mut self, line: &str) -> Result<(), SessionError> {
        self.apply(
            &line
                .parse::<Request>()
                .map_err(SessionError::ParseRequest)?,
        )?;
        self.pointer.frame();
        synchronize(&mut self.queue, &mut self.state)
    }

    fn timestamp(&self) -> u32 {
        let [lowest, second, third, fourth, ..] = self.start.elapsed().as_millis().to_le_bytes();
        u32::from_le_bytes([lowest, second, third, fourth])
    }

    fn apply(&self, request: &Request) -> Result<(), SessionError> {
        use wl_keyboard::KeyState;
        let time = self.timestamp();
        let Extent { width, height } = self.extent;
        match *request {
            Request::Move { x, y } => self.pointer.motion_absolute(time, x, y, width, height),
            Request::Key { ref name } => {
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
    seat: wl_seat::WlSeat,
    keymap: Keymap,
}

impl VirtualKeyboard {
    fn new(
        globals: &GlobalList,
        qh: &QueueHandle<ProtocolState>,
        seat: wl_seat::WlSeat,
        keymap: Keymap,
    ) -> Result<Self, SessionError> {
        let (file, size) = keymap.shared_file()?;
        let manager: ZwpVirtualKeyboardManagerV1 =
            globals.bind(qh, VIRTUAL_KEYBOARD_MANAGER_VERSIONS, ())?;
        let device = manager.create_virtual_keyboard(&seat, qh, ());
        let format = wl_keyboard::KeymapFormat::XkbV1.into();
        device.keymap(format, file.as_fd(), size);
        Ok(Self {
            device,
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
        self.seat.release();
    }
}
