// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell_core::geometry::Point;
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{DragAction, DragActions, ReceivedData, TransferData, TransferId};
use lavashell_core::units::Dp;
use lavashell_core::view::View;
use lavashell_platform::DragDevice;

#[derive(Debug)]
pub struct DropEvent {
    pub id: TransferId,
    pub surface: SurfaceId,
    pub position: Point,
    pub action: DragAction,
    pub data: Result<ReceivedData, Error>,
}

/// A pointer motion while the left button pressed on `surface` is held, passed to the drag source callback of the
/// app. When the callback returns a `DragRequest`, the drag starts and that button press creates no more gestures.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct DragGesture {
    pub surface: SurfaceId,
    pub origin: Point,
    pub position: Point,
}

impl DragGesture {
    /// Whether the pointer moved at least `distance` from `origin`, in logical pixels. A `distance` that is negative
    /// or not finite is never passed.
    #[must_use]
    pub fn passed_threshold(&self, distance: Dp) -> bool {
        distance.is_finite()
            && distance >= Dp::ZERO
            && (self.position.x - self.origin.x).hypot(self.position.y - self.origin.y) >= distance
    }
}

pub(crate) type DragCompletion = Box<dyn FnOnce(Result<DragAction, Error>, &mut View)>;
pub(crate) type DragStarted = Box<dyn FnOnce(DragDevice, &mut View)>;
pub(crate) type DragReceipt = Box<dyn FnMut(TransferId)>;

pub struct DragRequest {
    pub(crate) data: TransferData,
    pub(crate) actions: DragActions,
    pub(crate) done: DragCompletion,
    pub(crate) started: Option<DragStarted>,
    pub(crate) received: Option<DragReceipt>,
}

impl DragRequest {
    /// Creates a drag of `data` that allows `actions`. `done` runs with the view of the surface where the drag
    /// started. It reports the action only after every requested transfer was sent and the destination finished the
    /// drop. The drag never deletes files or app data. After a successful move, `done` removes the moved data itself.
    pub fn new(
        data: TransferData,
        actions: DragActions,
        done: impl FnOnce(Result<DragAction, Error>, &mut View) + 'static,
    ) -> Self {
        Self {
            data,
            actions,
            done: Box::new(done),
            started: None,
            received: None,
        }
    }

    /// Sets `started`, which runs with the view of the source surface once the platform reports that the drag
    /// started. A host that does not confirm the start of a drag reports it once the request to start went out.
    #[must_use]
    pub fn on_start(self, started: impl FnOnce(&mut View) + 'static) -> Self {
        self.on_start_with_device(move |_device, view| started(view))
    }

    pub(crate) fn on_start_with_device(
        mut self,
        started: impl FnOnce(DragDevice, &mut View) + 'static,
    ) -> Self {
        self.started = Some(Box::new(started));
        self
    }

    pub(crate) fn on_local_receive(mut self, received: impl FnMut(TransferId) + 'static) -> Self {
        self.received = Some(Box::new(received));
        self
    }
}
