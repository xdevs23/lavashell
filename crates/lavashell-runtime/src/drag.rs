// SPDX-License-Identifier: GPL-3.0-only
mod error;
mod request;
mod source;
mod target;
mod text;

use crate::pipe_transfers::{PipeTransfers, Transfer};
use crate::surface_records::SurfaceRecords;
use crate::watches::Watches;
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{DragEvent, DropAcceptance, ReceivedData, TransferId};
use lavashell_core::view::View;
use lavashell_platform::{
    DeviceEvent, DragSourceEvent, DragSourceId, DragSourceRequest, DragTargetEvent,
    DragTargetRequest, FormatId, PointerDevice, WatchId,
};
use source::DragSources;
use std::cell::RefCell;
use std::os::fd::OwnedFd;
use std::rc::Rc;
use std::sync::Arc;
use target::DragTargets;
use text::TextTransfers;

pub use error::Error;
pub use request::{DragGesture, DragRequest, DropEvent};

type DragObserver = Box<dyn FnMut(DragEvent, &mut View) -> Option<DropAcceptance>>;
type DropObserver = Box<dyn FnMut(DropEvent, &mut View) -> bool>;
type DragSourceObserver = Box<dyn FnMut(DragGesture, &mut View) -> Option<DragRequest>>;

/// A pipe transfer of a drag that ended, until `Drags::submit` reports it to the views.
enum Ended {
    Receipt(TransferId, Result<ReceivedData, Error>),
    Sent(DragSourceId, Result<(), Error>),
}

/// The parts that the drags offered to the app and the drags started by it both use: the built-in text drag and the
/// pipes that move the dragged bytes.
struct Shared {
    text: Option<TextTransfers>,
    pipes: PipeTransfers<Error>,
    ended: Rc<RefCell<Vec<Ended>>>,
}

impl Shared {
    /// Starts reading the dropped data of `id` in `format` for `surface` from `pipe`, and returns the watch of the
    /// pipe, or `None` when the read ended at once.
    fn receive(
        &self,
        id: TransferId,
        pipe: OwnedFd,
        surface: SurfaceId,
        format: FormatId,
    ) -> Option<WatchId> {
        let ended = Rc::clone(&self.ended);
        let done = Box::new(move |data| ended.borrow_mut().push(Ended::Receipt(id, data)));
        self.pipes
            .start(Transfer::receive(pipe, surface, format, done))
    }

    /// Starts writing `bytes` of the drag `id` to `pipe`, and returns the watch of the pipe, or `None` when the write
    /// ended at once.
    fn send(&self, id: DragSourceId, pipe: OwnedFd, bytes: Arc<[u8]>) -> Option<WatchId> {
        let ended = Rc::clone(&self.ended);
        let done = Box::new(move |result| ended.borrow_mut().push(Ended::Sent(id, result)));
        self.pipes.start(Transfer::send(pipe, bytes, done))
    }
}

/// The drag and drop of a run: the drags offered to the app, the drags started by it and the built-in text drag. A
/// platform executes the requests made by it and reports what happens to each drag. The runtime moves the dragged
/// bytes through pipes.
pub struct Drags {
    targets: DragTargets,
    sources: DragSources,
    shared: Shared,
}

impl Drags {
    /// Drag and drop whose pipes are watched through `watches`.
    #[must_use]
    pub fn new(watches: &Watches) -> Self {
        Self {
            targets: DragTargets::default(),
            sources: DragSources::default(),
            shared: Shared {
                text: None,
                pipes: PipeTransfers::new(watches),
                ended: Rc::default(),
            },
        }
    }

    /// Sets `observer`, which runs for each event of a drag offered to the app and returns the acceptance of the drag
    /// at its surface and position.
    pub fn on_drag(
        &mut self,
        observer: impl FnMut(DragEvent, &mut View) -> Option<DropAcceptance> + 'static,
    ) {
        self.targets.drag_observer = Some(Box::new(observer));
    }

    /// Sets `observer`, which runs once the data of a drop arrived or failed, and returns whether the app used it.
    pub fn on_drop(&mut self, observer: impl FnMut(DropEvent, &mut View) -> bool + 'static) {
        self.targets.drop_observer = Some(Box::new(observer));
    }

    /// Sets `observer`, which runs for a drag gesture and returns the drag to start from it.
    pub fn on_drag_source(
        &mut self,
        observer: impl FnMut(DragGesture, &mut View) -> Option<DragRequest> + 'static,
    ) {
        self.sources.observer = Some(Box::new(observer));
    }

    /// Enables the built-in drag and drop of text, which handles the drags that the observers registered by the app
    /// did not accept.
    pub fn enable_text_transfers(&mut self) {
        if self.shared.text.is_none() {
            self.shared.text = Some(TextTransfers::default());
        }
    }

    /// Reports to the views the drops whose data arrived and the transfers of started drags that ended.
    pub fn submit(&mut self, records: &mut SurfaceRecords) {
        let ended = std::mem::take(&mut *self.shared.ended.borrow_mut());
        for end in ended {
            match end {
                Ended::Receipt(id, data) => {
                    (self.targets).complete_drop(id, data, &mut self.shared, records);
                }
                Ended::Sent(id, result) => self.sources.sent(id, result, records),
            }
        }
    }

    /// Returns and removes the requests queued since the last call for the drags offered to the app.
    #[must_use]
    pub fn take_target_requests(&mut self) -> Vec<DragTargetRequest> {
        std::mem::take(&mut self.targets.requests)
    }

    /// Returns and removes the requests queued since the last call for the drags started by the app.
    #[must_use]
    pub fn take_source_requests(&mut self) -> Vec<DragSourceRequest> {
        std::mem::take(&mut self.sources.requests)
    }

    /// Applies what a platform reported about a drag offered to the app.
    pub fn handle_target(&mut self, event: DragTargetEvent, records: &mut SurfaceRecords) {
        self.targets.handle(event, &mut self.shared, records);
    }

    /// Applies what a platform reported about a drag started by the app.
    pub fn handle_source(&mut self, event: DragSourceEvent, records: &mut SurfaceRecords) {
        self.sources.handle(event, &self.shared, records);
    }

    /// Follows the drag gesture of the pointer in `event`, which happened on `surface`. A motion with the primary
    /// button down hands a gesture to the drag source observer registered by the app, and then to the built-in text
    /// drag. Returns whether the motion started a drag, so the view does not receive it.
    pub fn track_gesture(
        &mut self,
        surface: SurfaceId,
        event: &DeviceEvent,
        records: &mut SurfaceRecords,
    ) -> bool {
        (self.sources).track_gesture(surface, event, &self.shared, records)
    }

    /// Ends the drag gesture of the primary button of `pointer` on `surface`, whose interaction with the surface
    /// ended.
    pub fn cancel_gesture(&mut self, pointer: PointerDevice, surface: SurfaceId) {
        self.sources.cancel_gesture(pointer, surface);
    }

    /// Cancels the drops onto `surface` whose data is arriving, removes the drags over it, and cancels the drags
    /// started from it and the drag gestures on it, once the surface closed. It runs before the surface retires, so
    /// the view of the surface receives each cancel.
    pub fn forget_surface(&mut self, surface: SurfaceId, records: &mut SurfaceRecords) {
        self.shared.pipes.forget_surface(surface);
        self.submit(records);
        self.targets.forget_surface(surface, &mut self.shared);
        self.sources.forget_surface(surface, &self.shared, records);
    }

    /// Cancels every pipe transfer, reports the dropped data that ended with it, and ends every dropped drag and every
    /// started drag.
    pub fn close(&mut self, records: &mut SurfaceRecords) {
        self.shared.pipes.close();
        self.submit(records);
        self.targets.close(&mut self.shared, records);
        self.sources.close(&self.shared, records);
    }
}

#[cfg(test)]
mod tests;
