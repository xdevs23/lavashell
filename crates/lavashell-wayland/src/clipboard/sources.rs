// SPDX-License-Identifier: GPL-3.0-only
use super::{
    drag::{DragCompletion, DragReceipt, portable_action},
    pipes::Scope,
};
use crate::app::Platform;
use crate::{Cancellation, Error};
use lavashell_core::transfer::{DragAction, TransferData};
use sctk::{
    data_device_manager::{
        WritePipe,
        data_source::{CopyPasteSource, DataSourceHandler, DragSource},
    },
    reexports::client::{
        Connection, QueueHandle,
        protocol::{
            wl_data_device_manager::DndAction, wl_data_source::WlDataSource, wl_seat::WlSeat,
            wl_surface::WlSurface,
        },
    },
};
use smithay_client_toolkit as sctk;

pub(super) enum Source {
    Selection(CopyPasteSource),
    Drag(DragSource),
}

impl Source {
    pub fn inner(&self) -> &WlDataSource {
        match self {
            Self::Selection(source) => source.inner(),
            Self::Drag(source) => source.inner(),
        }
    }
}

pub(super) struct Payload {
    pub source: Source,
    pub data: TransferData,
    pub seat: WlSeat,
    pub drag: Option<Drag>,
}

pub(super) struct Drag {
    /// The surface that the drag started from.
    pub surface: WlSurface,
    pub progress: DragProgress,
}

pub(super) struct DragProgress {
    pub pending: usize,
    pub sent: usize,
    pub failure: Option<Error>,
    pub finished: bool,
    pub action: Option<DragAction>,
    pub done: DragCompletion,
    pub received: Option<DragReceipt>,
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

impl Platform {
    fn source_send(&mut self, source: &WlDataSource, mime_type: &str, pipe: WritePipe) {
        let incoming = self
            .clipboard
            .payloads
            .iter()
            .find(|payload| {
                payload.source.inner() == source
                    && payload
                        .drag
                        .as_ref()
                        .is_some_and(|drag| drag.progress.received.is_some())
            })
            .and_then(|payload| self.local_receiving_transfer(&pipe, &payload.seat));
        let Some(payload) = self
            .clipboard
            .payloads
            .iter_mut()
            .find(|payload| payload.source.inner() == source)
        else {
            return;
        };
        let Some(bytes) = payload.data.get(mime_type).cloned() else {
            if let Some(drag) = &mut payload.drag {
                drag.progress.failure = Some(Error::UnadvertisedFormat);
            }
            return;
        };
        if let Some(drag) = &mut payload.drag {
            drag.progress.pending += 1;
            if let Some(id) = incoming
                && let Some(received) = &mut drag.progress.received
            {
                received(id);
            }
        }
        let scope = Scope {
            seat: payload.seat.clone(),
            surface: payload.drag.as_ref().map(|drag| drag.surface.clone()),
            source: Some(source.clone()),
            incoming: None,
        };
        let source = source.clone();
        self.send_transfer(
            pipe,
            scope,
            bytes,
            Box::new(move |platform, result| {
                platform.source_sent(&source, result);
            }),
        );
    }

    fn source_sent(&mut self, source: &WlDataSource, result: Result<(), Error>) {
        let Some(drag) = self
            .clipboard
            .payloads
            .iter_mut()
            .find(|payload| payload.source.inner() == source)
            .and_then(|payload| payload.drag.as_mut())
        else {
            return;
        };
        let progress = &mut drag.progress;
        progress.pending -= 1;
        match result {
            Ok(()) => progress.sent += 1,
            Err(error) => progress.failure = Some(error),
        }
        self.finish_drag_source(source);
    }

    fn finish_drag_source(&mut self, source: &WlDataSource) {
        let Some(index) = self
            .clipboard
            .payloads
            .iter()
            .position(|payload| payload.source.inner() == source)
        else {
            return;
        };
        let result = self.clipboard.payloads[index]
            .drag
            .as_mut()
            .and_then(|drag| drag.progress.take_result());
        if let Some(result) = result
            && let Some(drag) = self.clipboard.payloads.remove(index).drag
        {
            self.complete_drag_request(&drag.surface, drag.progress.done, result);
        }
    }

    fn cancel_drag_source(&mut self, source: &WlDataSource, reason: Cancellation) {
        let Some(index) = self
            .clipboard
            .payloads
            .iter()
            .position(|payload| payload.source.inner() == source)
        else {
            return;
        };
        let payload = self.clipboard.payloads.remove(index);
        if let Some(drag) = payload.drag {
            let jobs: Vec<_> = self
                .clipboard
                .jobs
                .iter()
                .filter(|job| job.source.as_ref() == Some(source))
                .map(|job| job.id)
                .collect();
            for id in jobs {
                self.cancel_transfer(id, reason);
            }
            self.complete_drag_request(&drag.surface, drag.progress.done, Err(reason.into()));
        }
    }

    pub(crate) fn cancel_drag_sources(&mut self, seat: &WlSeat) {
        let sources: Vec<_> = self
            .clipboard
            .payloads
            .iter()
            .filter(|payload| &payload.seat == seat && payload.drag.is_some())
            .map(|payload| payload.source.inner().clone())
            .collect();
        for source in sources {
            self.cancel_drag_source(&source, Cancellation::InputDeviceGone);
        }
    }

    pub(crate) fn cancel_closed_drag_sources(&mut self) {
        let sources: Vec<_> = self
            .clipboard
            .payloads
            .iter()
            .filter(|payload| {
                (payload.drag.as_ref())
                    .is_some_and(|drag| !self.transfer_surface_live(&drag.surface))
            })
            .map(|payload| payload.source.inner().clone())
            .collect();
        for source in sources {
            self.cancel_drag_source(&source, Cancellation::SurfaceClosed);
        }
    }
}

impl DataSourceHandler for Platform {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }

    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        mime: String,
        pipe: WritePipe,
    ) {
        self.source_send(source, &mime, pipe);
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        self.cancel_drag_source(source, Cancellation::SourceCancelled);
    }

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if let Some(drag) = self
            .clipboard
            .payloads
            .iter_mut()
            .find(|payload| payload.source.inner() == source)
            .and_then(|payload| payload.drag.as_mut())
        {
            drag.progress.finished = true;
        }
        self.finish_drag_source(source);
    }

    fn action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        action: DndAction,
    ) {
        if let Some(drag) = self
            .clipboard
            .payloads
            .iter_mut()
            .find(|payload| payload.source.inner() == source)
            .and_then(|payload| payload.drag.as_mut())
        {
            drag.progress.action = portable_action(action);
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
            received: None,
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
