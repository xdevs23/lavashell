// SPDX-License-Identifier: GPL-3.0-only
use crate::transfer_pipe::{Progress, ReadBuffer, WriteBuffer};
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

pub(crate) enum Transfer<E> {
    Receive {
        pipe: File,
        surface: SurfaceId,
        format: FormatId,
        received: ReadBuffer,
        done: ReadCompletion<E>,
    },
    Send {
        pipe: File,
        sent: WriteBuffer,
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
            sent: WriteBuffer::new(bytes),
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
            Self::Send { pipe, sent, .. } => sent.write_to(pipe),
        }
    }

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

    pub fn forget_surface(&self, surface: SurfaceId) {
        let transfers: Vec<_> = (self.0.borrow_mut().transfers)
            .extract_if(.., |_, transfer| transfer.receives_for(surface))
            .collect();
        self.end(transfers, Cancellation::SurfaceClosed);
    }

    pub fn cancel(&self, watches: impl IntoIterator<Item = WatchId>, reason: Cancellation) {
        let transfers: Vec<_> = {
            let mut pipes = self.0.borrow_mut();
            (watches.into_iter())
                .filter_map(|watch| Some((watch, pipes.transfers.remove(&watch)?)))
                .collect()
        };
        self.end(transfers, reason);
    }

    pub fn close(&self) {
        let transfers = std::mem::take(&mut self.0.borrow_mut().transfers);
        self.end(transfers, Cancellation::AppClosed);
    }

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

fn watch_failure<E: From<Error> + From<Cancellation>>(error: watches::Error) -> E {
    match error {
        watches::Error::Cancelled(reason) => E::from(reason),
        exhausted @ watches::Error::NumberWatch(_) => E::from(Error::Watch(exhausted)),
    }
}

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
