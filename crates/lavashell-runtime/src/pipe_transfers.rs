// SPDX-License-Identifier: GPL-3.0-only
use crate::transfer_pipe::{Progress, ReadBuffer, write_available};
use crate::watches::{self, AfterReady, Watches};
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{Cancellation, ReceivedData};
use lavashell_platform::{FormatId, Readiness, WatchId};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::File;
use std::os::fd::OwnedFd;
use std::rc::{Rc, Weak};
use std::sync::Arc;

mod error;

pub use error::Error;

pub(crate) type ReadCompletion<E> = Box<dyn FnOnce(Result<ReceivedData, E>)>;
pub(crate) type WriteCompletion<E> = Box<dyn FnOnce(Result<(), E>)>;

/// The bytes that move through one pipe. `E` is the error of the subsystem that started the transfer.
pub(crate) enum Transfer<E> {
    /// Reads data in `format` for `surface` from the nonblocking pipe until the writer closes the pipe.
    Receive {
        pipe: File,
        surface: SurfaceId,
        format: FormatId,
        received: ReadBuffer,
        done: ReadCompletion<E>,
    },
    /// Writes `bytes` to the nonblocking pipe of a reader.
    Send {
        pipe: File,
        bytes: Arc<[u8]>,
        written: usize,
        done: WriteCompletion<E>,
    },
}

impl<E> Transfer<E> {
    pub fn receive(
        pipe: OwnedFd,
        surface: SurfaceId,
        format: FormatId,
        done: ReadCompletion<E>,
    ) -> Self {
        Self::Receive {
            pipe: File::from(pipe),
            surface,
            format,
            received: ReadBuffer::default(),
            done,
        }
    }

    pub fn send(pipe: OwnedFd, bytes: Arc<[u8]>, done: WriteCompletion<E>) -> Self {
        Self::Send {
            pipe: File::from(pipe),
            bytes,
            written: 0,
            done,
        }
    }

    fn receives_for(&self, surface: SurfaceId) -> bool {
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
                ..
            } => write_available(pipe, bytes, written),
        }
    }

    /// Ends the transfer and calls its completion with `result` and, for a read, the received bytes.
    fn finish(self, result: Result<(), E>) {
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
            Self::Send { done, .. } => done(result),
        }
    }
}

struct Pipes<E> {
    watches: Watches,
    transfers: BTreeMap<WatchId, Transfer<E>>,
}

/// The transfers through pipes that the runtime reads or writes, each under the id of the watch of its pipe. The
/// bytes move each time the platform reports a pipe as ready.
pub(crate) struct PipeTransfers<E>(Rc<RefCell<Pipes<E>>>);

impl<E> Clone for PipeTransfers<E> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<E: From<Error> + From<Cancellation> + 'static> PipeTransfers<E> {
    pub fn new(watches: &Watches) -> Self {
        Self(Rc::new(RefCell::new(Pipes {
            watches: watches.clone(),
            transfers: BTreeMap::new(),
        })))
    }

    /// Requests that the platform watch a copy of the pipe of `transfer`, stores the transfer until it ends, and
    /// returns the watch. A transfer whose pipe cannot be watched ends at once with the error and returns `None`.
    pub fn start(&self, transfer: Transfer<E>) -> Option<WatchId> {
        let watches = self.0.borrow().watches.clone();
        let weak_pipes = Rc::downgrade(&self.0);
        let started = (transfer.pipe().try_clone())
            .map_err(|source| E::from(Error::WatchPipe(source)))
            .and_then(|pipe| {
                let on_ready = move |watch| advance(&weak_pipes, watch);
                (watches.watch(pipe.into(), transfer.readiness(), on_ready)).map_err(watch_failure)
            });
        match started {
            Ok(watch) => {
                let _no_transfer_had_the_watch =
                    self.0.borrow_mut().transfers.insert(watch, transfer);
                Some(watch)
            }
            Err(error) => {
                transfer.finish(Err(error));
                None
            }
        }
    }

    /// Cancels the reads for `surface`, which closed.
    pub fn forget_surface(&self, surface: SurfaceId) {
        let transfers: Vec<_> = (self.0.borrow_mut().transfers)
            .extract_if(.., |_, transfer| transfer.receives_for(surface))
            .collect();
        self.end(transfers, Cancellation::SurfaceClosed);
    }

    /// Cancels the transfers watched under `watches` with `reason`. A transfer that ended already is skipped.
    pub fn cancel(&self, watches: impl IntoIterator<Item = WatchId>, reason: Cancellation) {
        let transfers: Vec<_> = {
            let mut pipes = self.0.borrow_mut();
            (watches.into_iter())
                .filter_map(|watch| Some((watch, pipes.transfers.remove(&watch)?)))
                .collect()
        };
        self.end(transfers, reason);
    }

    /// Cancels every transfer.
    pub fn close(&self) {
        let transfers = std::mem::take(&mut self.0.borrow_mut().transfers);
        self.end(transfers, Cancellation::AppClosed);
    }

    /// Ends `transfers` with `reason`, and requests the end of the watch of each.
    fn end(
        &self,
        transfers: impl IntoIterator<Item = (WatchId, Transfer<E>)>,
        reason: Cancellation,
    ) {
        let watches = self.0.borrow().watches.clone();
        for (watch, transfer) in transfers {
            watches.unwatch(watch);
            transfer.finish(Err(E::from(reason)));
        }
    }
}

/// Converts a failed watch into the caller error. A watch that fails because the watches are closed cancels the
/// transfer, like any other cancellation.
fn watch_failure<E: From<Error> + From<Cancellation>>(error: watches::Error) -> E {
    match error {
        watches::Error::Cancelled(reason) => E::from(reason),
        watches::Error::NumberWatch => E::from(Error::NumberWatch),
    }
}

/// Moves the bytes of the pipe watched under `watch`, and ends the transfer once the pipe ends.
fn advance<E: From<Error>>(pipes: &Weak<RefCell<Pipes<E>>>, watch: WatchId) -> AfterReady {
    let Some(pipes) = pipes.upgrade() else {
        return AfterReady::End;
    };
    let mut current = pipes.borrow_mut();
    let Some(transfer) = current.transfers.get_mut(&watch) else {
        return AfterReady::End;
    };
    let result = match transfer.move_available() {
        Ok(Progress::Pending) => return AfterReady::Keep,
        Ok(Progress::Complete) => Ok(()),
        Err(error) => Err(E::from(error)),
    };
    let ended = current.transfers.remove(&watch);
    drop(current);
    if let Some(transfer) = ended {
        transfer.finish(result);
    }
    AfterReady::End
}

#[cfg(test)]
mod tests;
