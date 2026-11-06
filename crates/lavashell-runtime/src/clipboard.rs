// SPDX-License-Identifier: GPL-3.0-only
mod text;

use crate::pipe_transfers::{PipeTransfers, ReadCompletion, Transfer};
use crate::{Error, SurfaceRecords, Watches};
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{Cancellation, ReceivedData, TransferData};
use lavashell_platform::{
    ClipboardEvent, ClipboardRequest, FormatId, ReceiveId, ReceiveRefusal, SelectionId,
    SelectionRefusal,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::os::fd::OwnedFd;
use std::rc::Rc;

pub use text::TextClipboard;

type SetCompletion = Box<dyn FnOnce(Result<(), Error>)>;

enum Request {
    Set(TransferData, SetCompletion),
    Read {
        surface: SurfaceId,
        formats: Vec<String>,
        done: ReadCompletion,
    },
}

impl Request {
    fn cancel(self, reason: Cancellation) {
        match self {
            Self::Set(_, done) => done(Err(reason.into())),
            Self::Read { done, .. } => done(Err(reason.into())),
        }
    }
}

struct Selection {
    data: TransferData,
    /// The completion of the request that set the selection, until the platform reports whether it set it.
    done: Option<SetCompletion>,
}

/// A read of the selection, until the platform starts it.
struct PendingRead {
    surface: SurfaceId,
    format: FormatId,
    done: ReadCompletion,
}

struct State {
    queued: VecDeque<Request>,
    closed: bool,
    /// The formats of the selection offered by the host.
    offered: Vec<FormatId>,
    selections: BTreeMap<SelectionId, Selection>,
    next_selection_id: u64,
    reads: BTreeMap<ReceiveId, PendingRead>,
    next_receive_id: u64,
    pipes: PipeTransfers,
    requests: Vec<ClipboardRequest>,
}

impl State {
    /// The first of `formats` found in the selection offered by the host.
    fn offered_format(&self, formats: &[String]) -> Result<FormatId, Error> {
        if self.offered.is_empty() {
            return Err(Error::ClipboardEmpty);
        }
        (formats.iter())
            .find_map(|format| (self.offered.iter()).find(|offered| offered.0 == *format))
            .cloned()
            .ok_or(Error::NoRequestedFormat)
    }

    fn new_selection_id(&mut self) -> Result<SelectionId, Error> {
        let id = SelectionId(self.next_selection_id);
        self.next_selection_id = id.0.checked_add(1).ok_or(Error::NumberSelection)?;
        Ok(id)
    }

    fn new_receive_id(&mut self) -> Result<ReceiveId, Error> {
        let id = ReceiveId(self.next_receive_id);
        self.next_receive_id = id.0.checked_add(1).ok_or(Error::NumberReceive)?;
        Ok(id)
    }
}

/// The handle through which apps and views put data on the clipboard and read data from it.
#[derive(Clone)]
pub struct Clipboard(Rc<RefCell<State>>);

impl Clipboard {
    /// Offers every representation in `data` as the clipboard selection, binary media, documents and URI lists
    /// included. `done` reports whether the platform set the selection. Another client that pastes reads the data
    /// from this app, so the app has to run until that client received it.
    pub fn set_data(&self, data: TransferData, done: impl FnOnce(Result<(), Error>) + 'static) {
        self.enqueue(Request::Set(data, Box::new(done)));
    }

    /// Reads the clipboard selection in the first of `formats` offered by the selection, for `surface`. The buffer for
    /// the bytes grows as they arrive. The read fails when `surface` closes before the bytes arrived.
    pub fn read(
        &self,
        surface: SurfaceId,
        formats: impl IntoIterator<Item = impl Into<String>>,
        done: impl FnOnce(Result<ReceivedData, Error>) + 'static,
    ) {
        self.enqueue(Request::Read {
            surface,
            formats: formats.into_iter().map(Into::into).collect(),
            done: Box::new(done),
        });
    }

    fn enqueue(&self, request: Request) {
        let mut state = self.0.borrow_mut();
        if !state.closed {
            state.queued.push_back(request);
            return;
        }
        drop(state);
        request.cancel(Cancellation::AppClosed);
    }
}

/// The clipboard of a run: the requests made through `Clipboard`, the selections set by the app, the formats offered
/// by the host, the reads of the selection, and the pipes that the runtime reads or writes. A platform executes the
/// requests made by it and reports what happens to the clipboard. The runtime moves the bytes through each pipe each
/// time the platform reports the pipe as ready.
pub struct ClipboardTransfers(Rc<RefCell<State>>);

impl ClipboardTransfers {
    /// Clipboard transfers whose pipes are watched through `watches`.
    #[must_use]
    pub fn new(watches: &Watches) -> Self {
        Self(Rc::new(RefCell::new(State {
            queued: VecDeque::new(),
            closed: false,
            offered: Vec::new(),
            selections: BTreeMap::new(),
            next_selection_id: 0,
            reads: BTreeMap::new(),
            next_receive_id: 0,
            pipes: PipeTransfers::new(watches),
            requests: Vec::new(),
        })))
    }

    /// A handle that puts data on the clipboard and reads data from it.
    #[must_use]
    pub fn clipboard(&self) -> Clipboard {
        Clipboard(self.0.clone())
    }

    /// Starts the requests made through `Clipboard`, in the order in which they were made. A read for a surface that
    /// is not open in `records` fails.
    pub fn submit(&self, records: &SurfaceRecords) {
        let queued = std::mem::take(&mut self.0.borrow_mut().queued);
        for request in queued {
            match request {
                Request::Set(data, done) => self.set_selection(data, done),
                Request::Read {
                    surface,
                    formats,
                    done,
                } if records.is_open(surface) => self.receive(surface, &formats, done),
                Request::Read { done, .. } => done(Err(Cancellation::SurfaceClosed.into())),
            }
        }
    }

    /// Takes the clipboard requests made since the last call, for the platform to execute.
    #[must_use]
    pub fn take_requests(&self) -> Vec<ClipboardRequest> {
        std::mem::take(&mut self.0.borrow_mut().requests)
    }

    /// Applies what a platform reported about the clipboard.
    pub fn handle(&self, event: ClipboardEvent) {
        match event {
            ClipboardEvent::SelectionOffered(formats) => self.0.borrow_mut().offered = formats,
            ClipboardEvent::SelectionSet(id, result) => self.selection_set(id, result),
            ClipboardEvent::ReceiveStarted(id, result) => self.receive_started(id, result),
            ClipboardEvent::SourceSend(id, format, pipe) => self.send(id, &format, pipe),
            ClipboardEvent::SourceCancelled(id) => {
                let cancelled = self.0.borrow_mut().selections.remove(&id);
                drop(cancelled);
            }
        }
    }

    /// Cancels the reads for `surface`, which retired.
    pub fn forget_surface(&self, surface: SurfaceId) {
        let mut state = self.0.borrow_mut();
        let reads: Vec<_> = (state.reads)
            .extract_if(.., |_, read| read.surface == surface)
            .map(|(_, read)| read.done)
            .collect();
        let pipes = state.pipes.clone();
        drop(state);
        for done in reads {
            done(Err(Cancellation::SurfaceClosed.into()));
        }
        pipes.forget_surface(surface);
    }

    /// Cancels every queued request, every read, every transfer and every request made after that. Each selection not
    /// yet set by the platform reports the cancellation.
    pub fn close(&self) {
        let mut state = self.0.borrow_mut();
        state.closed = true;
        let queued = std::mem::take(&mut state.queued);
        let selections = std::mem::take(&mut state.selections);
        let reads = std::mem::take(&mut state.reads);
        let pipes = state.pipes.clone();
        let requests = std::mem::take(&mut state.requests);
        drop(state);
        drop(requests);
        for request in queued {
            request.cancel(Cancellation::AppClosed);
        }
        for done in selections
            .into_values()
            .filter_map(|selection| selection.done)
        {
            done(Err(Cancellation::AppClosed.into()));
        }
        for read in reads.into_values() {
            (read.done)(Err(Cancellation::AppClosed.into()));
        }
        pipes.close();
    }

    fn set_selection(&self, data: TransferData, done: SetCompletion) {
        let mut state = self.0.borrow_mut();
        let id = match state.new_selection_id() {
            Ok(id) => id,
            Err(error) => {
                drop(state);
                done(Err(error));
                return;
            }
        };
        let formats = data
            .formats()
            .map(|format| FormatId(format.to_owned()))
            .collect();
        state
            .requests
            .push(ClipboardRequest::SetSelection(id, formats));
        let selection = Selection {
            data,
            done: Some(done),
        };
        let _no_selection_had_the_id = state.selections.insert(id, selection);
    }

    fn selection_set(&self, id: SelectionId, result: Result<(), SelectionRefusal>) {
        let mut state = self.0.borrow_mut();
        let done = match result {
            Ok(()) => (state.selections.get_mut(&id)).and_then(|selection| selection.done.take()),
            Err(_) => (state.selections.remove(&id)).and_then(|selection| selection.done),
        };
        drop(state);
        if let Some(done) = done {
            done(result.map_err(Error::SetSelection));
        }
    }

    /// Asks the platform to read the selection in the first of `formats` offered by the host.
    fn receive(&self, surface: SurfaceId, formats: &[String], done: ReadCompletion) {
        let mut state = self.0.borrow_mut();
        let started = state.offered_format(formats).and_then(|format| {
            let id = state.new_receive_id()?;
            Ok((id, format))
        });
        match started {
            Ok((id, format)) => {
                state
                    .requests
                    .push(ClipboardRequest::Receive(id, format.clone()));
                let read = PendingRead {
                    surface,
                    format,
                    done,
                };
                let _no_read_had_the_id = state.reads.insert(id, read);
            }
            Err(error) => {
                drop(state);
                done(Err(error));
            }
        }
    }

    /// Starts the transfer through `pipe` for the read `id`, or ends the read with the refusal of the platform. A
    /// pipe for a read that ended meanwhile closes at once.
    fn receive_started(&self, id: ReceiveId, result: Result<OwnedFd, ReceiveRefusal>) {
        let read = self.0.borrow_mut().reads.remove(&id);
        let Some(PendingRead {
            surface,
            format,
            done,
        }) = read
        else {
            return;
        };
        match result {
            Ok(pipe) => {
                let pipes = self.0.borrow().pipes.clone();
                let _watched = pipes.start(Transfer::receive(pipe, surface, format, done));
            }
            Err(refusal) => done(Err(read_error(refusal))),
        }
    }

    /// Writes the selection `id` in `format` to `pipe`. When the selection has no such format, `pipe` closes at once.
    fn send(&self, id: SelectionId, format: &FormatId, pipe: OwnedFd) {
        let bytes = (self.0.borrow().selections.get(&id))
            .and_then(|selection| selection.data.get(&format.0))
            .cloned();
        let Some(bytes) = bytes else {
            return;
        };
        let pipes = self.0.borrow().pipes.clone();
        let _watched = pipes.start(Transfer::send(
            pipe,
            bytes,
            Box::new(|result| {
                if let Err(error) = result {
                    log_failure("the clipboard selection was not sent", &error);
                }
            }),
        ));
    }
}

/// The failure of a read that the platform did not start.
fn read_error(refusal: ReceiveRefusal) -> Error {
    match refusal {
        ReceiveRefusal::Empty => Error::ClipboardEmpty,
        ReceiveRefusal::NoFormat => Error::NoRequestedFormat,
        ReceiveRefusal::Pipe(source) => Error::CreatePipe(source),
    }
}

/// Logs that `message` happened because of `error`: a cancellation at debug level, any other failure at warn level.
fn log_failure(message: &str, error: &Error) {
    match error {
        Error::Cancelled(_) => log::debug!("{message}: {error}"),
        _ => log::warn!("{message}: {error}"),
    }
}

#[cfg(test)]
pub(crate) mod tests;
