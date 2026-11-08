// SPDX-License-Identifier: GPL-3.0-only
use super::Translator;
use crate::Error;
use lavashell_actor::{Closed, Intake, Receiver};
use lavashell_platform::PlatformRequest;
use sctk::reexports::calloop::generic::Generic;
use sctk::reexports::calloop::ping::Ping;
use sctk::reexports::calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use sctk::reexports::client::backend::WaylandError;
use smithay_client_toolkit as sctk;
use std::io;
use std::os::fd::{AsFd, BorrowedFd};

/// The intake of requests from the runtime. The intake closes while the socket of the connection cannot accept more
/// data.
pub(crate) struct Requests {
    intake: Intake<PlatformRequest>,
    /// The event source that runs the requests once they arrive.
    source: RegistrationToken,
    /// The event source that wakes the loop once the socket is writable again, while the intake is closed.
    writable: Option<RegistrationToken>,
}

/// How serving ends: without a failure once every sender of the requests is gone, or with the first failure.
#[derive(Default)]
pub(crate) struct Ending(Option<Result<(), Error>>);

impl Ending {
    /// Records `outcome`. A failure replaces an end without a failure, and a later failure never replaces the first.
    pub(crate) fn record(&mut self, outcome: Result<(), Error>) {
        if !matches!(self.0, Some(Err(_))) {
            self.0 = Some(outcome);
        }
    }

    /// Removes and returns the recorded outcome.
    pub(crate) fn take(&mut self) -> Option<Result<(), Error>> {
        self.0.take()
    }
}

/// Runs `execute` on each request that arrived, in order. Returns how serving ends: with the failure of a request,
/// or once every sender of the requests is gone and no request waits.
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
        // The last sender may have gone while these requests waited. The wake sent when it went was already used up by
        // this removal, so this wake lets the next removal see that every sender is gone.
        wake.ping();
    }
    arrived.into_iter().try_for_each(execute).err().map(Err)
}

impl Translator {
    /// Executes `request` through the executor for that kind of request.
    pub(super) fn execute(&mut self, request: PlatformRequest) -> Result<(), Error> {
        match request {
            PlatformRequest::Surface(request) => self.execute_surface(request),
            PlatformRequest::Panel(request) => self.execute_panel(request),
            PlatformRequest::Input(request) => {
                self.execute_input(request);
                Ok(())
            }
            PlatformRequest::WindowControl(request) => self.execute_window_control(request),
            PlatformRequest::Watch(request) => self.execute_watch(request),
            PlatformRequest::Clipboard(request) => {
                self.execute_clipboard(request);
                Ok(())
            }
            PlatformRequest::DragTarget(request) => {
                self.execute_drag_target(request);
                Ok(())
            }
            PlatformRequest::DragSource(request) => {
                self.execute_drag_source(request);
                Ok(())
            }
            PlatformRequest::Lock(request) => self.execute_lock(request),
            PlatformRequest::FocusGrab(request) => self.execute_focus_grab(request),
            PlatformRequest::Accessibility(request) => {
                self.execute_accessibility(request);
                Ok(())
            }
        }
    }

    /// Sends the requests written on this thread to the compositor. While the socket accepts no more data, the
    /// requests of the runtime wait in their mailbox, and they are run again once the socket accepted the rest.
    pub(super) fn flush(&mut self) -> Result<(), Error> {
        match self.connection.flush() {
            Ok(()) => self.requests.open(&self.loop_handle),
            Err(WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => self
                .requests
                .close(&self.loop_handle, self.connection.as_fd()),
            Err(error) => Err(Error::Flush(error)),
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

    /// Closes the intake and runs no more requests until `socket` is writable.
    fn close<D>(
        &mut self,
        handle: &LoopHandle<'static, D>,
        socket: BorrowedFd<'_>,
    ) -> Result<(), Error> {
        if self.writable.is_some() {
            return Ok(());
        }
        self.intake.close();
        handle.disable(&self.source).map_err(Error::PauseRequests)?;
        // The event loop watches the socket for reading already, and a file descriptor joins it only once.
        let duplicate = socket
            .try_clone_to_owned()
            .map_err(Error::DuplicateSocket)?;
        let writable = Generic::new(duplicate, Interest::WRITE, Mode::Level);
        let token = handle
            .insert_source(writable, |_readiness, _socket, _data| {
                Ok(PostAction::Continue)
            })
            .map_err(|failure| Error::WatchWritable(failure.error))?;
        self.writable = Some(token);
        Ok(())
    }

    /// Opens the intake and runs requests again, once the socket accepted everything written before.
    fn open<D>(&mut self, handle: &LoopHandle<'static, D>) -> Result<(), Error> {
        let Some(writable) = self.writable.take() else {
            return Ok(());
        };
        handle.remove(writable);
        handle.enable(&self.source).map_err(Error::ResumeRequests)?;
        self.intake.open();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
