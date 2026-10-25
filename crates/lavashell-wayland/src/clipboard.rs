// SPDX-License-Identifier: GPL-3.0-only
//! The clipboard selection and drag and drop between the app and other Wayland clients. Both pick a MIME type
//! from the types offered by the other side. Both move the data through nonblocking pipes watched by the event
//! loop.

mod drag;
mod pipes;
mod selection;
mod sources;
mod state;
mod text;
#[cfg(test)]
#[path = "text_transfer/tests.rs"]
mod text_transfer_tests;

pub(crate) use drag::DragCompletion;
pub use drag::{DragEvent, DragGesture, DragOfferInfo, DragRequest, DropEvent, TransferId};
pub(super) use state::ClipboardState;

use crate::{Cancellation, Error};
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{ReceivedData, TransferData};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

type SetCompletion = Box<dyn FnOnce(Result<(), Error>)>;
type ReadCompletion = Box<dyn FnOnce(Result<ReceivedData, Error>)>;

enum Request {
    Set(TransferData, SetCompletion),
    Read {
        surface: SurfaceId,
        formats: Vec<String>,
        done: ReadCompletion,
    },
}

#[derive(Clone, Default)]
pub struct Clipboard(Rc<RefCell<Queue>>);

#[derive(Default)]
struct Queue {
    requests: VecDeque<Request>,
    closed: bool,
}

impl Queue {
    fn enqueue(queue: &Rc<RefCell<Self>>, request: Request) {
        let mut state = queue.borrow_mut();
        if !state.closed {
            state.requests.push_back(request);
            return;
        }
        drop(state);
        Self::reject(request);
    }

    fn reject(request: Request) {
        match request {
            Request::Set(_, done) => done(Err(Cancellation::AppClosed.into())),
            Request::Read { done, .. } => done(Err(Cancellation::AppClosed.into())),
        }
    }
}

impl Clipboard {
    /// Offers every representation in `data` as the clipboard selection, binary media, documents and URI lists
    /// included. `done` reports whether the selection request was sent to the compositor. Another client that
    /// pastes reads the data from this app, so the app has to run until that client received it.
    pub fn set_data(&self, data: TransferData, done: impl FnOnce(Result<(), Error>) + 'static) {
        Queue::enqueue(&self.0, Request::Set(data, Box::new(done)));
    }

    /// Reads the clipboard selection in the first of `formats` offered by the selection, through the seat with input
    /// focus on `surface`. The buffer for the bytes grows as they arrive.
    pub fn read(
        &self,
        surface: SurfaceId,
        formats: impl IntoIterator<Item = impl Into<String>>,
        done: impl FnOnce(Result<ReceivedData, Error>) + 'static,
    ) {
        Queue::enqueue(
            &self.0,
            Request::Read {
                surface,
                formats: formats.into_iter().map(Into::into).collect(),
                done: Box::new(done),
            },
        );
    }
}
