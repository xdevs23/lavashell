// SPDX-License-Identifier: GPL-3.0-only
use super::{ReadCompletion, State, log_failure};
use crate::Error;
use crate::transfer_pipe::{Progress, ReadBuffer, write_available};
use crate::watches::AfterReady;
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::ReceivedData;
use lavashell_platform::{FormatId, Readiness, WatchId};
use std::cell::RefCell;
use std::fs::File;
use std::os::fd::OwnedFd;
use std::rc::{Rc, Weak};
use std::sync::Arc;

/// The bytes that move through one pipe.
pub(super) enum Transfer {
    /// Reads the selection in `format` from the nonblocking pipe until the client that writes it closes the pipe.
    Receive {
        pipe: File,
        surface: SurfaceId,
        format: FormatId,
        received: ReadBuffer,
        done: ReadCompletion,
    },
    /// Writes `bytes` to the nonblocking pipe of a client that pastes the selection.
    Send {
        pipe: File,
        bytes: Arc<[u8]>,
        written: usize,
    },
}

impl Transfer {
    pub fn receive(
        pipe: OwnedFd,
        surface: SurfaceId,
        format: FormatId,
        done: ReadCompletion,
    ) -> Self {
        Self::Receive {
            pipe: File::from(pipe),
            surface,
            format,
            received: ReadBuffer::default(),
            done,
        }
    }

    pub fn send(pipe: OwnedFd, bytes: Arc<[u8]>) -> Self {
        Self::Send {
            pipe: File::from(pipe),
            bytes,
            written: 0,
        }
    }

    pub fn is_for(&self, surface: SurfaceId) -> bool {
        matches!(self, Self::Receive { surface: target, .. } if *target == surface)
    }

    fn pipe(&self) -> &File {
        match self {
            Self::Receive { pipe, .. } | Self::Send { pipe, .. } => pipe,
        }
    }

    fn readiness(&self) -> Readiness {
        match self {
            Self::Receive { .. } => Readiness::Readable,
            Self::Send { .. } => Readiness::Writable,
        }
    }

    fn move_available(&mut self) -> Result<Progress, Error> {
        match self {
            Self::Receive { pipe, received, .. } => received.read_from(pipe),
            Self::Send {
                pipe,
                bytes,
                written,
            } => write_available(pipe, bytes, written),
        }
    }

    /// Ends the transfer with `result`. A read reports the bytes or the failure to its caller. A failed write is
    /// logged, because the client that pastes sees only the end of the pipe.
    pub fn finish(self, result: Result<(), Error>) {
        match self {
            Self::Receive {
                format,
                received,
                done,
                ..
            } => done(result.map(|()| ReceivedData {
                mime_type: format.0,
                bytes: received.into_bytes(),
            })),
            Self::Send { .. } => {
                if let Err(error) = result {
                    log_failure("the clipboard selection was not sent", &error);
                }
            }
        }
    }
}

/// Asks the platform to watch a copy of the pipe of `transfer`, and stores the transfer in `state` under the id of the
/// watch until the transfer ends.
pub(super) fn start(state: &Rc<RefCell<State>>, transfer: Transfer) {
    let watches = state.borrow().watches.clone();
    let weak_state = Rc::downgrade(state);
    let started = (transfer.pipe().try_clone())
        .map_err(Error::WatchPipe)
        .and_then(|pipe| {
            let on_ready = move |watch| advance(&weak_state, watch);
            watches.watch(pipe.into(), transfer.readiness(), on_ready)
        });
    match started {
        Ok(watch) => {
            let _no_transfer_had_the_watch = state.borrow_mut().transfers.insert(watch, transfer);
        }
        Err(error) => transfer.finish(Err(error)),
    }
}

/// Moves the bytes that the pipe watched under `watch` takes or holds, and ends the transfer once the pipe ends.
fn advance(state: &Weak<RefCell<State>>, watch: WatchId) -> AfterReady {
    let Some(state) = state.upgrade() else {
        return AfterReady::End;
    };
    let mut current = state.borrow_mut();
    let Some(transfer) = current.transfers.get_mut(&watch) else {
        return AfterReady::End;
    };
    let result = match transfer.move_available() {
        Ok(Progress::Pending) => return AfterReady::Keep,
        Ok(Progress::Complete) => Ok(()),
        Err(error) => Err(error),
    };
    let ended = current.transfers.remove(&watch);
    drop(current);
    if let Some(transfer) = ended {
        transfer.finish(result);
    }
    AfterReady::End
}
