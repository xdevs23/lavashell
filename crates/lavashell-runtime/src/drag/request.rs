// SPDX-License-Identifier: GPL-3.0-only
use super::Error;
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

#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct DragGesture {
    pub surface: SurfaceId,
    pub origin: Point,
    pub position: Point,
}

impl DragGesture {
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
