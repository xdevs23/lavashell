// SPDX-License-Identifier: GPL-3.0-only
use super::request::{DragCompletion, DragReceipt, DragStarted};
use super::{DragGesture, DragRequest, DragSourceObserver, Shared};
use crate::{Error, SurfaceRecords};
use lavashell_core::geometry::Point;
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{Cancellation, DragAction, TransferData, TransferId};
use lavashell_platform::{
    Button, DeviceEvent, DragSourceEvent, DragSourceId, DragSourceRequest, FormatId, PointerDevice,
    PressState, WatchId,
};
use std::collections::BTreeMap;
use std::io;
use std::os::fd::OwnedFd;

/// The first button pressed by a pointer on a surface, while it is down.
struct Gesture {
    surface: SurfaceId,
    button: Button,
    origin: Point,
}

/// A drag started by the app, from the request to start it until its result is reported.
struct Source {
    surface: SurfaceId,
    data: TransferData,
    started: Option<DragStarted>,
    received: Option<DragReceipt>,
    progress: DragProgress,
    /// The watches of the pipes to which the data of the drag was written.
    sends: Vec<WatchId>,
}

impl Source {
    /// Reports `result` to the view of the surface where the drag started. The drag fails when that surface closed.
    fn complete(self, result: Result<DragAction, Error>, records: &mut SurfaceRecords) {
        let result = if records.is_open(self.surface) {
            result
        } else {
            Err(Cancellation::SurfaceClosed.into())
        };
        match records.view_mut(self.surface) {
            Some(view) => (self.progress.done)(result, view),
            None => log::warn!(
                "a drag ended after its source surface was removed, so its result is not reported"
            ),
        }
    }

    /// Cancels the running transfers of the drag with `reason`, and reports the cancel to the view.
    fn cancel(self, reason: Cancellation, shared: &Shared, records: &mut SurfaceRecords) {
        shared.pipes.cancel(self.sends.iter().copied(), reason);
        self.complete(Err(reason.into()), records);
    }
}

struct DragProgress {
    pending: usize,
    sent: usize,
    failure: Option<Error>,
    finished: bool,
    action: Option<DragAction>,
    done: DragCompletion,
}

impl DragProgress {
    /// The result of the drag, or `None` until the destination finished the drop and every requested transfer
    /// ended. The drag fails when a transfer failed or when nothing was sent.
    fn take_result(&mut self) -> Option<Result<DragAction, Error>> {
        if !self.finished || self.pending != 0 {
            return None;
        }
        Some(match (self.failure.take(), self.action, self.sent) {
            (Some(error), _, _) => Err(error),
            (None, Some(action), sent) if sent > 0 => Ok(action),
            _ => Err(Error::DragWithoutTransfer),
        })
    }
}

/// The drags started by the app: the drag source observer registered by the app, the drag gesture of each pointer
/// and the started drags until their result is reported.
#[derive(Default)]
pub(super) struct DragSources {
    pub observer: Option<DragSourceObserver>,
    gestures: BTreeMap<PointerDevice, Gesture>,
    sources: BTreeMap<DragSourceId, Source>,
    next_id: u64,
    pub requests: Vec<DragSourceRequest>,
}

impl DragSources {
    pub fn track_gesture(
        &mut self,
        surface: SurfaceId,
        event: &DeviceEvent,
        shared: &Shared,
        records: &mut SurfaceRecords,
    ) -> bool {
        match *event {
            DeviceEvent::Button {
                pointer,
                button,
                state: PressState::Pressed,
                at,
                ..
            } => {
                let _first_press = self.gestures.entry(pointer).or_insert(Gesture {
                    surface,
                    button,
                    origin: at,
                });
                false
            }
            DeviceEvent::Button {
                pointer,
                button,
                state: PressState::Released,
                ..
            } => {
                self.end_gesture(pointer, |gesture| {
                    gesture.button == button && gesture.surface == surface
                });
                false
            }
            DeviceEvent::PointerMotion { pointer, at } => {
                self.drag_from(pointer, at, shared, records)
            }
            _ => false,
        }
    }

    pub fn cancel_gesture(&mut self, pointer: PointerDevice, surface: SurfaceId) {
        self.end_gesture(pointer, |gesture| {
            gesture.button == Button::Primary && gesture.surface == surface
        });
    }

    pub fn handle(
        &mut self,
        event: DragSourceEvent,
        shared: &Shared,
        records: &mut SurfaceRecords,
    ) {
        match event {
            DragSourceEvent::Started(id, Ok(device)) => {
                let Some(source) = self.sources.get_mut(&id) else {
                    return;
                };
                let Some(started) = source.started.take() else {
                    return;
                };
                match records.open_view_mut(source.surface) {
                    Some(view) => started(device, view),
                    None => log::warn!("a drag started after its source surface closed"),
                }
            }
            DragSourceEvent::Started(id, Err(refusal)) => {
                self.end_source(id, Err(Error::StartDrag(refusal)), records);
            }
            DragSourceEvent::Send {
                drag,
                format,
                pipe,
                local,
            } => self.send(drag, &format, pipe, local, shared, records),
            DragSourceEvent::ActionSelected(id, action) => {
                if let Some(source) = self.sources.get_mut(&id) {
                    source.progress.action = action;
                }
            }
            DragSourceEvent::Finished(id) => {
                if let Some(source) = self.sources.get_mut(&id) {
                    source.progress.finished = true;
                }
                self.finish_source(id, records);
            }
            DragSourceEvent::Cancelled(id, reason) => {
                if let Some(source) = self.sources.remove(&id) {
                    source.cancel(reason, shared, records);
                }
            }
        }
    }

    /// Cancels the drags started from `surface` and the drag gestures on it, once the surface closed.
    pub fn forget_surface(
        &mut self,
        surface: SurfaceId,
        shared: &Shared,
        records: &mut SurfaceRecords,
    ) {
        self.gestures
            .retain(|_, gesture| gesture.surface != surface);
        let sources: Vec<_> = (self.sources)
            .extract_if(.., |_, source| source.surface == surface)
            .collect();
        for (id, source) in sources {
            self.requests.push(DragSourceRequest::Cancel(id));
            source.cancel(Cancellation::SurfaceClosed, shared, records);
        }
    }

    /// Ends every started drag, because the app closes.
    pub fn close(&mut self, shared: &Shared, records: &mut SurfaceRecords) {
        for source in std::mem::take(&mut self.sources).into_values() {
            source.cancel(Cancellation::AppClosed, shared, records);
        }
    }

    /// Records the end of a transfer of the drag `id`, and reports the result of the drag once it is known.
    pub fn sent(
        &mut self,
        id: DragSourceId,
        result: Result<(), Error>,
        records: &mut SurfaceRecords,
    ) {
        let Some(source) = self.sources.get_mut(&id) else {
            return;
        };
        let progress = &mut source.progress;
        progress.pending -= 1;
        match result {
            Ok(()) => progress.sent += 1,
            Err(error) => progress.failure = Some(error),
        }
        self.finish_source(id, records);
    }

    fn end_gesture(&mut self, pointer: PointerDevice, ends: impl FnOnce(&Gesture) -> bool) {
        if self.gestures.get(&pointer).is_some_and(ends) {
            let _ended = self.gestures.remove(&pointer);
        }
    }

    /// Hands the gesture of `pointer`, which moved to `at`, to the drag source observers, and starts the drag that
    /// they return. A gesture that starts a drag ends.
    fn drag_from(
        &mut self,
        pointer: PointerDevice,
        at: Point,
        shared: &Shared,
        records: &mut SurfaceRecords,
    ) -> bool {
        let primary = |gesture: &&Gesture| gesture.button == Button::Primary;
        let Some(gesture) = self.gestures.get(&pointer).filter(primary) else {
            return false;
        };
        let gesture = DragGesture {
            surface: gesture.surface,
            origin: gesture.origin,
            position: at,
        };
        let Some(request) = self.dispatch_drag_source(gesture, shared, records) else {
            return false;
        };
        let _started = self.gestures.remove(&pointer);
        self.start(pointer, gesture.surface, request, records);
        true
    }

    fn dispatch_drag_source(
        &mut self,
        gesture: DragGesture,
        shared: &Shared,
        records: &mut SurfaceRecords,
    ) -> Option<DragRequest> {
        let view = records.open_view_mut(gesture.surface)?;
        (self.observer.as_mut())
            .and_then(|observer| observer(gesture, view))
            .or_else(|| (shared.text.as_ref()).and_then(|text| text.source(&gesture, view)))
    }

    /// Requests that the platform start the drag of `request` from `surface` with `pointer`.
    fn start(
        &mut self,
        pointer: PointerDevice,
        surface: SurfaceId,
        request: DragRequest,
        records: &mut SurfaceRecords,
    ) {
        let DragRequest {
            data,
            actions,
            done,
            started,
            received,
        } = request;
        let source = Source {
            surface,
            data,
            started,
            received,
            progress: DragProgress {
                pending: 0,
                sent: 0,
                failure: None,
                finished: false,
                action: None,
                done,
            },
            sends: Vec::new(),
        };
        if actions.is_empty() {
            source.complete(Err(Error::DragWithoutAction), records);
            return;
        }
        let id = match self.new_source_id() {
            Ok(id) => id,
            Err(error) => {
                source.complete(Err(error), records);
                return;
            }
        };
        let formats = (source.data.formats())
            .map(|format| FormatId(format.to_owned()))
            .collect();
        self.requests.push(DragSourceRequest::Start {
            drag: id,
            pointer,
            surface,
            formats,
            actions,
        });
        let _no_drag_had_the_id = self.sources.insert(id, source);
    }

    fn new_source_id(&mut self) -> Result<DragSourceId, Error> {
        let id = DragSourceId(self.next_id);
        self.next_id = id.0.checked_add(1).ok_or(Error::NumberDragSource)?;
        Ok(id)
    }

    /// Writes the data of the drag `id` in `format` to `pipe`. A format that the drag did not offer or a pipe that the
    /// platform could not prepare makes the drag fail.
    fn send(
        &mut self,
        id: DragSourceId,
        format: &FormatId,
        pipe: io::Result<OwnedFd>,
        local: Option<TransferId>,
        shared: &Shared,
        records: &mut SurfaceRecords,
    ) {
        let Some(source) = self.sources.get_mut(&id) else {
            return;
        };
        let transfer = (source.data.get(&format.0).cloned())
            .ok_or(Error::UnadvertisedFormat)
            .and_then(|bytes| pipe.map(|pipe| (pipe, bytes)).map_err(Error::SendDrag));
        match transfer {
            Ok((pipe, bytes)) => {
                source.progress.pending += 1;
                if let Some(local) = local
                    && let Some(received) = &mut source.received
                {
                    received(local);
                }
                source.sends.extend(shared.send(id, pipe, bytes));
            }
            Err(error) => {
                source.progress.failure = Some(error);
                self.finish_source(id, records);
            }
        }
    }

    fn finish_source(&mut self, id: DragSourceId, records: &mut SurfaceRecords) {
        let result = (self.sources.get_mut(&id)).and_then(|source| source.progress.take_result());
        if let Some(result) = result {
            self.end_source(id, result, records);
        }
    }

    fn end_source(
        &mut self,
        id: DragSourceId,
        result: Result<DragAction, Error>,
        records: &mut SurfaceRecords,
    ) {
        if let Some(source) = self.sources.remove(&id) {
            source.complete(result, records);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress() -> DragProgress {
        DragProgress {
            pending: 1,
            sent: 0,
            failure: None,
            finished: false,
            action: Some(DragAction::Move),
            done: Box::new(|_, _| {}),
        }
    }

    #[test]
    fn physical_drop_and_finish_cannot_complete_move_before_transmission() {
        let mut progress = progress();
        assert!(progress.take_result().is_none());
        progress.finished = true;
        assert!(progress.take_result().is_none());
        progress.pending = 0;
        assert!(matches!(
            progress.take_result(),
            Some(Err(Error::DragWithoutTransfer))
        ));
        progress.sent = 1;
        assert!(matches!(progress.take_result(), Some(Ok(DragAction::Move))));
    }

    #[test]
    fn successful_send_requires_finish_and_failed_send_prevents_move() {
        let mut progress = progress();
        progress.pending = 0;
        progress.sent = 1;
        assert!(progress.take_result().is_none());
        progress.finished = true;
        progress.failure = Some(Error::WritePipe(std::io::ErrorKind::BrokenPipe.into()));
        assert!(matches!(
            progress.take_result(),
            Some(Err(Error::WritePipe(_)))
        ));
        progress.action = None;
        assert!(matches!(
            progress.take_result(),
            Some(Err(Error::DragWithoutTransfer))
        ));
    }
}
