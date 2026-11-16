// SPDX-License-Identifier: GPL-3.0-only
mod error;
mod request;
mod source;
mod target;
mod text;

use crate::pipe_transfers::{PipeTransfers, Transfer};
use crate::registry::Subsystem;
use crate::surface_records::SurfaceRecords;
use crate::watches::Watches;
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{DragEvent, DropAcceptance, ReceivedData, TransferId};
use lavashell_core::view::View;
use lavashell_platform::{
    Capability, DeviceEvent, DragSourceEvent, DragSourceId, DragSourceRequest, DragTargetEvent,
    DragTargetRequest, FormatId, PointerDevice, WatchId,
};
use source::DragSources;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::os::fd::OwnedFd;
use std::rc::Rc;
use std::sync::Arc;
use target::DragTargets;
use text::TextTransfers;

pub use error::Error;
pub use request::{DragGesture, DragRequest, DropEvent};

inventory::submit! { Subsystem {
    capability: Capability::DragAndDrop,
    used: |setup| setup.drags.observed(),
} }

type DragObserver = Box<dyn FnMut(DragEvent, &mut View) -> Option<DropAcceptance>>;
type DropObserver = Box<dyn FnMut(DropEvent, &mut View) -> bool>;
type DragSourceObserver = Box<dyn FnMut(DragGesture, &mut View) -> Option<DragRequest>>;

enum Ended {
    Receipt(TransferId, Result<ReceivedData, Error>),
    Sent(DragSourceId, Result<(), Error>),
}

struct Shared {
    text: Option<TextTransfers>,
    pipes: PipeTransfers<Error>,
    ended: Rc<RefCell<Vec<Ended>>>,
}

impl Shared {
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

    fn send(&self, id: DragSourceId, pipe: OwnedFd, bytes: Arc<[u8]>) -> Option<WatchId> {
        let ended = Rc::clone(&self.ended);
        let done = Box::new(move |result| ended.borrow_mut().push(Ended::Sent(id, result)));
        self.pipes.start(Transfer::send(pipe, bytes, done))
    }
}

pub(crate) struct Drags {
    targets: DragTargets,
    sources: DragSources,
    shared: Shared,
}

impl Drags {
    #[must_use]
    pub(crate) fn new(watches: &Watches) -> Self {
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

    pub(crate) fn on_drag(
        &mut self,
        observer: impl FnMut(DragEvent, &mut View) -> Option<DropAcceptance> + 'static,
    ) {
        self.targets.drag_observer = Some(Box::new(observer));
    }

    pub(crate) fn on_drop(&mut self, observer: impl FnMut(DropEvent, &mut View) -> bool + 'static) {
        self.targets.drop_observer = Some(Box::new(observer));
    }

    pub(crate) fn on_drag_source(
        &mut self,
        observer: impl FnMut(DragGesture, &mut View) -> Option<DragRequest> + 'static,
    ) {
        self.sources.observer = Some(Box::new(observer));
    }

    fn observed(&self) -> bool {
        self.targets.drag_observer.is_some()
            || self.targets.drop_observer.is_some()
            || self.sources.observer.is_some()
    }

    pub(crate) fn enable_text_transfers(&mut self) {
        let _enabled_text_transfers = self.shared.text.get_or_insert_default();
    }

    pub(crate) fn restrict_to(&mut self, offered: &BTreeSet<Capability>) {
        if !offered.contains(&Capability::DragAndDrop) {
            self.shared.text = None;
        }
    }

    pub(crate) fn submit(&mut self, records: &mut SurfaceRecords) {
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

    #[must_use]
    pub(crate) fn take_target_requests(&mut self) -> Vec<DragTargetRequest> {
        std::mem::take(&mut self.targets.requests)
    }

    #[must_use]
    pub(crate) fn take_source_requests(&mut self) -> Vec<DragSourceRequest> {
        std::mem::take(&mut self.sources.requests)
    }

    pub(crate) fn handle_target(&mut self, event: DragTargetEvent, records: &mut SurfaceRecords) {
        self.targets.handle(event, &mut self.shared, records);
    }

    pub(crate) fn handle_source(&mut self, event: DragSourceEvent, records: &mut SurfaceRecords) {
        self.sources.handle(event, &self.shared, records);
    }

    pub(crate) fn track_gesture(
        &mut self,
        surface: SurfaceId,
        event: &DeviceEvent,
        records: &mut SurfaceRecords,
    ) -> bool {
        (self.sources).track_gesture(surface, event, &self.shared, records)
    }

    pub(crate) fn cancel_gesture(&mut self, pointer: PointerDevice, surface: SurfaceId) {
        self.sources.cancel_gesture(pointer, surface);
    }

    pub(crate) fn forget_surface(&mut self, surface: SurfaceId, records: &mut SurfaceRecords) {
        self.shared.pipes.forget_surface(surface);
        self.submit(records);
        self.targets.forget_surface(surface, &mut self.shared);
        self.sources.forget_surface(surface, &self.shared, records);
    }

    pub(crate) fn close(&mut self, records: &mut SurfaceRecords) {
        self.shared.pipes.close();
        self.submit(records);
        self.targets.close(&mut self.shared, records);
        self.sources.close(&self.shared, records);
    }
}

#[cfg(test)]
mod tests;
