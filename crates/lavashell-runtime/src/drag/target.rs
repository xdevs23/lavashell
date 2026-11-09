// SPDX-License-Identifier: GPL-3.0-only
use super::{DragObserver, DropEvent, DropObserver, Error, Shared};
use crate::surface_records::SurfaceRecords;
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{
    Cancellation, DragEvent, DragOfferInfo, DropAcceptance, ReceivedData, TransferId,
};
use lavashell_platform::{
    DragDevice, DragReceiveRefusal, DragTargetEvent, DragTargetRequest, FormatId, WatchId,
};
use std::collections::BTreeMap;
use std::os::fd::OwnedFd;

/// A drag offered to the app, while it is over a surface.
struct Offer {
    device: DragDevice,
    info: DragOfferInfo,
    /// The acceptance returned for the last enter or motion.
    acceptance: Option<DropAcceptance>,
}

/// A dropped drag, until its data arrived.
struct Dropped {
    info: DragOfferInfo,
    choice: DropAcceptance,
    /// The watch of the pipe through which the data arrives, once the platform started the read.
    receive: Option<WatchId>,
}

/// The drags offered to the app: the observers registered by the app, the drags over a surface and the dropped drags
/// whose data has not arrived yet.
#[derive(Default)]
pub(super) struct DragTargets {
    pub drag_observer: Option<DragObserver>,
    pub drop_observer: Option<DropObserver>,
    offers: BTreeMap<TransferId, Offer>,
    dropped: BTreeMap<TransferId, Dropped>,
    pub requests: Vec<DragTargetRequest>,
}

impl DragTargets {
    pub fn handle(
        &mut self,
        event: DragTargetEvent,
        shared: &mut Shared,
        records: &mut SurfaceRecords,
    ) {
        match event {
            DragTargetEvent::Enter { device, offer } => {
                let entered = Offer {
                    device,
                    info: offer.clone(),
                    acceptance: None,
                };
                let _no_offer_had_the_id = self.offers.insert(offer.id, entered);
                self.negotiate(DragEvent::Enter(offer), device, shared, records);
            }
            DragTargetEvent::Motion(id, at) => {
                self.renegotiate(id, |info| info.position = at, shared, records);
            }
            DragTargetEvent::ActionsChanged(id, actions) => {
                self.renegotiate(id, |info| info.actions = actions, shared, records);
            }
            DragTargetEvent::ActionSelected(id, action) => {
                if let Some(offer) = self.offers.get_mut(&id) {
                    offer.info.selected_action = action;
                }
            }
            DragTargetEvent::Leave(id) => {
                let left = |id, surface| DragEvent::Leave { id, surface };
                self.end_offer(id, shared, records, left);
            }
            DragTargetEvent::Cancelled(id) => {
                let cancelled = |id, surface| DragEvent::Cancelled { id, surface };
                self.end_offer(id, shared, records, cancelled);
                self.cancel_drop(id, Cancellation::InputDeviceGone, shared, records);
            }
            DragTargetEvent::Drop(id) => self.drop_offer(id, shared, records),
            DragTargetEvent::ReceiveStarted(id, pipe) => {
                self.receive_started(id, pipe, shared, records);
            }
        }
    }

    /// Removes the drags over `surface`, which closed, and the state of the built-in text drag for them.
    pub fn forget_surface(&mut self, surface: SurfaceId, shared: &mut Shared) {
        for (id, _) in (self.offers).extract_if(.., |_, offer| offer.info.surface == surface) {
            if let Some(text) = &mut shared.text {
                text.forget(id);
            }
        }
    }

    /// Ends every dropped drag whose data has not arrived, because the app closes.
    pub fn close(&mut self, shared: &mut Shared, records: &mut SurfaceRecords) {
        let dropped: Vec<_> = self.dropped.keys().copied().collect();
        for id in dropped {
            self.cancel_drop(id, Cancellation::AppClosed, shared, records);
        }
    }

    /// Hands `event` to the view and returns the acceptance that the offer allows to the platform. The new answer
    /// replaces the answer for the offer that the platform did not execute yet.
    fn negotiate(
        &mut self,
        event: DragEvent,
        device: DragDevice,
        shared: &mut Shared,
        records: &mut SurfaceRecords,
    ) {
        let id = event.id();
        let acceptance = self.dispatch_drag_event(event, device, shared, records);
        let Some(offer) = self.offers.get_mut(&id) else {
            return;
        };
        offer.acceptance =
            acceptance.filter(|choice| choice.accepts(&offer.info.mime_types, offer.info.actions));
        let answer = offer.acceptance.clone();
        (self.requests).retain(
            |request| !matches!(request, DragTargetRequest::Answer(queued, _) if *queued == id),
        );
        self.requests.push(DragTargetRequest::Answer(id, answer));
    }

    /// Applies `change` to the offer `id` and negotiates it again as a motion.
    fn renegotiate(
        &mut self,
        id: TransferId,
        change: impl FnOnce(&mut DragOfferInfo),
        shared: &mut Shared,
        records: &mut SurfaceRecords,
    ) {
        let Some(offer) = self.offers.get_mut(&id) else {
            return;
        };
        change(&mut offer.info);
        let (info, device) = (offer.info.clone(), offer.device);
        self.negotiate(DragEvent::Motion(info), device, shared, records);
    }

    /// Removes the offer `id`, which ended before a drop, and reports the event made by `ended` to the view.
    fn end_offer(
        &mut self,
        id: TransferId,
        shared: &mut Shared,
        records: &mut SurfaceRecords,
        ended: impl FnOnce(TransferId, SurfaceId) -> DragEvent,
    ) {
        if let Some(offer) = self.offers.remove(&id) {
            let event = ended(id, offer.info.surface);
            self.end_drag_event(event, offer.device, shared, records);
        }
    }

    /// Reports the drop to the view, and requests the data from the platform when the last acceptance allows the drop.
    /// Otherwise the view receives a cancel and the platform releases the drag.
    fn drop_offer(&mut self, id: TransferId, shared: &mut Shared, records: &mut SurfaceRecords) {
        let Some(Offer {
            device,
            info,
            acceptance,
        }) = self.offers.remove(&id)
        else {
            return;
        };
        self.end_drag_event(DragEvent::Drop(info.clone()), device, shared, records);
        let choice = acceptance.filter(|choice| {
            info.selected_action == Some(choice.action) && records.is_open(info.surface)
        });
        let Some(choice) = choice else {
            let cancelled = DragEvent::Cancelled {
                id,
                surface: info.surface,
            };
            self.end_drag_event(cancelled, device, shared, records);
            self.requests.push(DragTargetRequest::Release(id));
            return;
        };
        let format = FormatId(choice.mime_type.clone());
        self.requests.push(DragTargetRequest::Receive(id, format));
        let dropped = Dropped {
            info,
            choice,
            receive: None,
        };
        let _no_drop_had_the_id = self.dropped.insert(id, dropped);
    }

    /// Reads the dropped data of `id` from `pipe`, or ends the drop with the refusal of the platform.
    fn receive_started(
        &mut self,
        id: TransferId,
        pipe: Result<OwnedFd, DragReceiveRefusal>,
        shared: &mut Shared,
        records: &mut SurfaceRecords,
    ) {
        let Some(dropped) = self.dropped.get_mut(&id) else {
            return;
        };
        match pipe {
            Ok(pipe) => {
                let format = FormatId(dropped.choice.mime_type.clone());
                dropped.receive = shared.receive(id, pipe, dropped.info.surface, format);
            }
            Err(refusal) => {
                self.complete_drop(id, Err(Error::ReceiveDrag(refusal)), shared, records);
            }
        }
    }

    /// Ends the dropped drag `id` with `reason`, and cancels the read of its data.
    fn cancel_drop(
        &mut self,
        id: TransferId,
        reason: Cancellation,
        shared: &mut Shared,
        records: &mut SurfaceRecords,
    ) {
        let receive = self.dropped.get(&id).and_then(|dropped| dropped.receive);
        shared.pipes.cancel(receive, reason);
        self.complete_drop(id, Err(reason.into()), shared, records);
    }

    /// Hands the dropped data of `id` or its failure to the view, and finishes the drop when the view used the data.
    pub fn complete_drop(
        &mut self,
        id: TransferId,
        data: Result<ReceivedData, Error>,
        shared: &mut Shared,
        records: &mut SurfaceRecords,
    ) {
        let Some(Dropped { info, choice, .. }) = self.dropped.remove(&id) else {
            return;
        };
        let received = data.is_ok() && records.is_open(info.surface);
        let event = DropEvent {
            id,
            surface: info.surface,
            position: info.position,
            action: choice.action,
            data,
        };
        let used = self.dispatch_drop_event(event, shared, records);
        let request = if received && used {
            DragTargetRequest::Finish(id)
        } else {
            DragTargetRequest::Release(id)
        };
        self.requests.push(request);
    }

    /// Hands `event` to the built-in text drag when it accepted the drop, and to the observer registered by the app
    /// otherwise. Returns whether the view used the data. The drop onto a surface that closed fails.
    fn dispatch_drop_event(
        &mut self,
        mut event: DropEvent,
        shared: &mut Shared,
        records: &mut SurfaceRecords,
    ) -> bool {
        if !records.is_open(event.surface) {
            event.data = Err(Cancellation::SurfaceClosed.into());
        }
        let Some(view) = records.view_mut(event.surface) else {
            return false;
        };
        if let Some(text) = &mut shared.text
            && text.has_pending(event.id)
        {
            return text.receive(event, view);
        }
        (self.drop_observer.as_mut()).is_some_and(|observer| observer(event, view))
    }

    /// Reports to the view a drag that left the surface, was dropped on it or was cancelled over it. An acceptance
    /// returned for such a drag has no effect and is only logged.
    fn end_drag_event(
        &mut self,
        event: DragEvent,
        device: DragDevice,
        shared: &mut Shared,
        records: &mut SurfaceRecords,
    ) {
        if let Some(acceptance) = self.dispatch_drag_event(event, device, shared, records) {
            log::warn!(
                "the acceptance of {} came for a drag that already ended, so it is ignored",
                acceptance.mime_type
            );
        }
    }

    /// Hands `event` to the observer registered by the app and the built-in text drag, and returns the acceptance of
    /// the drag. The text drag handles an enter or a motion that the app did not accept, and every later event of the
    /// drag that it accepted.
    fn dispatch_drag_event(
        &mut self,
        event: DragEvent,
        device: DragDevice,
        shared: &mut Shared,
        records: &mut SurfaceRecords,
    ) -> Option<DropAcceptance> {
        let view = records.open_view_mut(event.surface())?;
        if !matches!(&event, DragEvent::Enter(_) | DragEvent::Motion(_)) {
            if let Some(text) = &mut shared.text
                && text.owns(&event)
            {
                return text.drag(&event, device, view);
            }
            return (self.drag_observer.as_mut()).and_then(|observer| observer(event, view));
        }
        let accepted =
            (self.drag_observer.as_mut()).and_then(|observer| observer(event.clone(), view));
        if let Some(text) = &mut shared.text {
            if accepted.is_some() {
                text.application_accepted(&event);
            } else {
                return text.drag(&event, device, view);
            }
        }
        accepted
    }
}
