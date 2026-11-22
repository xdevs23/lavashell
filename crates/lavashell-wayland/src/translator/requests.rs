// SPDX-License-Identifier: GPL-3.0-only
use super::Translator;
use crate::ConnectionError;
use crate::Error;
use crate::sctk;
use lavashell_actor::{Closed, Intake, Receiver};
use lavashell_platform::PlatformRequest;
use sctk::reexports::calloop::generic::Generic;
use sctk::reexports::calloop::ping::Ping;
use sctk::reexports::calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use sctk::reexports::client::backend::WaylandError;
use std::io;
use std::os::fd::{AsFd, BorrowedFd};

mod error;

pub use error::RequestsError;

pub(crate) struct Requests {
    intake: Intake<PlatformRequest>,
    source: RegistrationToken,
    writable: Option<RegistrationToken>,
}

#[derive(Default)]
pub(crate) struct Ending(Option<Result<(), Error>>);

impl Ending {
    pub(crate) fn record(&mut self, outcome: Result<(), Error>) {
        if !matches!(self.0, Some(Err(_))) {
            self.0 = Some(outcome);
        }
    }

    pub(crate) fn take(&mut self) -> Option<Result<(), Error>> {
        self.0.take()
    }
}

pub(super) fn execute_arrived(
    requests: &Receiver<PlatformRequest>,
    wake: &Ping,
    execute: impl FnMut(PlatformRequest) -> Result<(), Error>,
) -> Option<Result<(), Error>> {
    let arrived = match requests.take_arrived() {
        Ok(arrived) => arrived,
        Err(Closed) => return Some(Ok(())),
    };
    if !arrived.is_empty() {
        wake.ping();
    }
    arrived.into_iter().try_for_each(execute).err().map(Err)
}

impl Translator {
    pub(super) fn execute(&mut self, request: PlatformRequest) -> Result<(), Error> {
        match request {
            PlatformRequest::Surface(request) => self.execute_surface(request),
            PlatformRequest::Panel(request) => self.execute_panel(request).map_err(Error::from),
            PlatformRequest::Input(request) => {
                self.execute_input(request);
                Ok(())
            }
            PlatformRequest::WindowControl(request) => self.execute_window_control(request),
            PlatformRequest::Watch(request) => (self.watches)
                .execute(&self.loop_handle, request, &self.view)
                .map_err(Error::from),
            PlatformRequest::Clipboard(request) => {
                self.execute_clipboard(request);
                Ok(())
            }
            PlatformRequest::DragTarget(request) => {
                self.execute_drag_target(request);
                Ok(())
            }
            PlatformRequest::DragSource(request) => {
                self.execute_drag_source(request).map_err(Error::from)
            }
            PlatformRequest::Lock(request) => (self.protocols)
                .execute_lock(request, &self.connection, &self.qh)
                .map_err(Error::from),
            PlatformRequest::FocusGrab(request) => {
                self.execute_focus_grab(request).map_err(Error::from)
            }
            PlatformRequest::Accessibility(request) => {
                self.execute_accessibility(request);
                Ok(())
            }
        }
    }

    pub(super) fn flush(&mut self) -> Result<(), Error> {
        match self.connection.flush() {
            Ok(()) => self.requests.open(&self.loop_handle).map_err(Error::from),
            Err(WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => self
                .requests
                .close(&self.loop_handle, self.connection.as_fd())
                .map_err(Error::from),
            Err(error) => Err(Error::from(ConnectionError::Flush(error))),
        }
    }
}

impl Requests {
    pub(super) fn new(intake: Intake<PlatformRequest>, source: RegistrationToken) -> Self {
        Self {
            intake,
            source,
            writable: None,
        }
    }

    fn close<D>(
        &mut self,
        handle: &LoopHandle<'static, D>,
        socket: BorrowedFd<'_>,
    ) -> Result<(), RequestsError> {
        if self.writable.is_some() {
            return Ok(());
        }
        self.intake.close();
        (handle.disable(&self.source))
            .map_err(|error| RequestsError::PauseRequests(error.into()))?;
        let duplicate = socket
            .try_clone_to_owned()
            .map_err(RequestsError::DuplicateSocket)?;
        let writable = Generic::new(duplicate, Interest::WRITE, Mode::Level);
        let token = handle
            .insert_source(writable, |_readiness, _socket, _data| {
                Ok(PostAction::Continue)
            })
            .map_err(|failure| RequestsError::WatchWritable(failure.into()))?;
        self.writable = Some(token);
        Ok(())
    }

    fn open<D>(&mut self, handle: &LoopHandle<'static, D>) -> Result<(), RequestsError> {
        let Some(writable) = self.writable.take() else {
            return Ok(());
        };
        handle.remove(writable);
        (handle.enable(&self.source))
            .map_err(|error| RequestsError::ResumeRequests(error.into()))?;
        self.intake.open();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
