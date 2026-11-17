// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod error;
mod frame;
mod next_steps;
mod png_file;
pub mod protocol;
mod state;

use error::SessionError;
use frame::{CapturedFrame, PendingFrame};
use lavashell_error_chain::message_chain;
use next_steps::next_step;
pub use frame::PixelFormat;
use protocol::{Reply, Request};
use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::reexports::client::{
    Connection, EventQueue, globals::registry_queue_init, protocol::wl_output,
};
use smithay_client_toolkit::reexports::protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;
use smithay_client_toolkit::registry::RegistryState;
use smithay_client_toolkit::shm::Shm;
use state::ProtocolState;
use std::io::{BufRead, Write};
use std::ops::RangeInclusive;
use std::path::Path;
use std::process::ExitCode;

// https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-screencopy-unstable-v1.xml
const SCREENCOPY_MANAGER_VERSIONS: RangeInclusive<u32> = 3..=3;
const WITHOUT_CURSOR: i32 = 0;

#[must_use]
pub fn run_capture_session() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}\n{}", message_chain(&error), next_step(&error));
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), SessionError> {
    let mut session = Session::open()?;
    for line in std::io::stdin().lock().lines() {
        let line = line.map_err(SessionError::ReadRequest)?;
        match session.serve(&line) {
            Ok(frame) => send_reply(&Reply::from(frame))?,
            Err(failure) => {
                send_reply(&Reply::Failed {
                    message: message_chain(&failure),
                })?;
                return Err(failure);
            }
        }
    }
    session.close()
}

fn send_reply(reply: &Reply) -> Result<(), SessionError> {
    let line = serde_json::to_string(reply).map_err(SessionError::EncodeReply)?;
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{line}")
        .and_then(|()| stdout.flush())
        .map_err(SessionError::SendReply)
}

struct Session {
    connection: Connection,
    queue: EventQueue<ProtocolState>,
    state: ProtocolState,
    manager: ZwlrScreencopyManagerV1,
    output: wl_output::WlOutput,
}

impl Session {
    fn open() -> Result<Self, SessionError> {
        let connection = Connection::connect_to_env()?;
        let (globals, mut queue) = registry_queue_init::<ProtocolState>(&connection)?;
        let qh = queue.handle();
        let mut state = ProtocolState {
            registry: RegistryState::new(&globals),
            outputs: OutputState::new(&globals, &qh),
            shm: Shm::bind(&globals, &qh)?,
            pool: None,
            frame: PendingFrame::default(),
        };
        let _events_already_applied_to_the_state = queue.roundtrip(&mut state)?;
        let output = state.only_output()?;
        let manager = globals.bind(&qh, SCREENCOPY_MANAGER_VERSIONS, ())?;
        Ok(Self {
            connection,
            queue,
            state,
            manager,
            output,
        })
    }

    fn serve(&mut self, line: &str) -> Result<CapturedFrame, SessionError> {
        match line
            .parse::<Request>()
            .map_err(SessionError::ParseRequest)?
        {
            Request::CaptureChange { image } => self.capture_change(&image),
        }
    }

    fn capture_change(&mut self, image: &Path) -> Result<CapturedFrame, SessionError> {
        let frame =
            (self.manager).capture_output(WITHOUT_CURSOR, &self.output, &self.queue.handle(), ());
        let outcome = loop {
            if let Some(outcome) = self.state.frame.take_outcome() {
                break outcome;
            }
            let _events_already_applied_to_the_state =
                self.queue.blocking_dispatch(&mut self.state)?;
        };
        frame.destroy();
        std::mem::take(&mut self.state.frame).release();
        let captured = outcome?;
        self.state.write_image(captured, image)?;
        Ok(captured)
    }

    fn close(self) -> Result<(), SessionError> {
        self.manager.destroy();
        self.connection.flush()?;
        Ok(())
    }
}
