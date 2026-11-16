// SPDX-License-Identifier: GPL-3.0-only
mod error;
mod text;

use crate::pipe_transfers::{PipeTransfers, ReadCompletion, Transfer};
use crate::registry::Subsystem;
use crate::surface_records::SurfaceRecords;
use crate::view_actions::ViewActions;
use crate::watches::Watches;
use lavashell_core::id_counter::IdCounter;
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{Cancellation, ReceivedData, TransferData};
use lavashell_platform::{
    Capability, ClipboardEvent, ClipboardRequest, FormatId, ReceiveId, ReceiveRefusal, SelectionId,
    SelectionRefusal,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io;
use std::os::fd::OwnedFd;
use std::rc::Rc;

pub use error::Error;

pub(crate) use text::{ShortcutOutcome, TextClipboard};

inventory::submit! { Subsystem {
    capability: Capability::Clipboard,
    used: |setup| setup.clipboard.used(),
} }

type SetCompletion = Box<dyn FnOnce(Result<(), Error>)>;

enum Request {
    Set(TransferData, SetCompletion),
    Read {
        surface: SurfaceId,
        formats: Vec<String>,
        done: ReadCompletion<Error>,
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
    done: Option<SetCompletion>,
}

struct PendingRead {
    surface: SurfaceId,
    format: FormatId,
    done: ReadCompletion<Error>,
}

struct State {
    queued: VecDeque<Request>,
    closed: bool,
    taken: bool,
    offered: Vec<FormatId>,
    selections: BTreeMap<SelectionId, Selection>,
    selection_ids: IdCounter,
    reads: BTreeMap<ReceiveId, PendingRead>,
    receive_ids: IdCounter,
    pipes: PipeTransfers<Error>,
    requests: Vec<ClipboardRequest>,
}

impl State {
    fn offered_format(&self, formats: &[String]) -> Result<FormatId, Error> {
        if self.offered.is_empty() {
            return Err(Error::Empty);
        }
        (formats.iter())
            .find_map(|format| (self.offered.iter()).find(|offered| offered.0 == *format))
            .cloned()
            .ok_or(Error::NoRequestedFormat)
    }

    fn new_selection_id(&mut self) -> Result<SelectionId, Error> {
        let id = self
            .selection_ids
            .next_id()
            .map_err(Error::NumberSelection)?;
        Ok(SelectionId(id))
    }

    fn new_receive_id(&mut self) -> Result<ReceiveId, Error> {
        let id = self.receive_ids.next_id().map_err(Error::NumberReceive)?;
        Ok(ReceiveId(id))
    }
}

#[derive(Clone)]
pub struct Clipboard(Rc<RefCell<State>>);

impl Clipboard {
    pub fn set_data(&self, data: TransferData, done: impl FnOnce(Result<(), Error>) + 'static) {
        self.enqueue(Request::Set(data, Box::new(done)));
    }

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

pub(crate) struct ClipboardTransfers(Rc<RefCell<State>>);

impl ClipboardTransfers {
    #[must_use]
    pub(crate) fn new(watches: &Watches) -> Self {
        Self(Rc::new(RefCell::new(State {
            queued: VecDeque::new(),
            closed: false,
            taken: false,
            offered: Vec::new(),
            selections: BTreeMap::new(),
            selection_ids: IdCounter::default(),
            reads: BTreeMap::new(),
            receive_ids: IdCounter::default(),
            pipes: PipeTransfers::new(watches),
            requests: Vec::new(),
        })))
    }

    #[must_use]
    pub(crate) fn clipboard(&self) -> Clipboard {
        self.0.borrow_mut().taken = true;
        Clipboard(self.0.clone())
    }

    pub(crate) fn used(&self) -> bool {
        self.0.borrow().taken
    }

    pub(crate) fn text_clipboard(
        &self,
        actions: ViewActions,
        offered: &BTreeSet<Capability>,
    ) -> Option<TextClipboard> {
        (offered.contains(&Capability::Clipboard))
            .then(|| TextClipboard::new(Clipboard(self.0.clone()), actions))
    }

    pub(crate) fn submit(&self, records: &SurfaceRecords) {
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

    #[must_use]
    pub(crate) fn take_requests(&self) -> Vec<ClipboardRequest> {
        std::mem::take(&mut self.0.borrow_mut().requests)
    }

    pub(crate) fn handle(&self, event: ClipboardEvent) {
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

    pub(crate) fn forget_surface(&self, surface: SurfaceId) {
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

    pub(crate) fn close(&self) {
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

    fn receive(&self, surface: SurfaceId, formats: &[String], done: ReadCompletion<Error>) {
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

    fn send(&self, id: SelectionId, format: &FormatId, pipe: io::Result<OwnedFd>) {
        let pipe = match pipe {
            Ok(pipe) => pipe,
            Err(source) => {
                log_failure(
                    "the clipboard selection was not sent",
                    &Error::CreatePipe(source),
                );
                return;
            }
        };
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

fn read_error(refusal: ReceiveRefusal) -> Error {
    match refusal {
        ReceiveRefusal::Empty => Error::Empty,
        ReceiveRefusal::NoFormat => Error::NoRequestedFormat,
        ReceiveRefusal::Pipe(source) => Error::CreatePipe(source),
    }
}

fn log_failure(message: &str, error: &Error) {
    match error {
        Error::Cancelled(_) => log::debug!("{message}: {error}"),
        _ => log::warn!("{message}: {error}"),
    }
}

#[cfg(test)]
pub(crate) mod tests;
