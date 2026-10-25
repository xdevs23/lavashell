// SPDX-License-Identifier: GPL-3.0-only
use super::{Clipboard, drag::Incoming, pipes::Job, sources::Payload};
use crate::Cancellation;
use crate::app::Platform;
use sctk::data_device_manager::{DataDeviceManagerState, data_device::DataDevice};
use sctk::reexports::client::{
    Proxy, QueueHandle,
    globals::GlobalList,
    protocol::{wl_seat::WlSeat, wl_surface::WlSurface},
};
use smithay_client_toolkit as sctk;

pub(super) struct PointerGrab {
    pub surface: WlSurface,
    pub serial: u32,
    pub button: u32,
    pub origin: lavashell_core::geometry::Point,
}

pub(super) struct SeatTransfers {
    pub seat: WlSeat,
    pub device: DataDevice,
    pub grab: Option<PointerGrab>,
}

pub(crate) struct ClipboardState {
    pub(super) manager: Option<DataDeviceManagerState>,
    pub(super) devices: Vec<SeatTransfers>,
    pub(super) serial: Option<(WlSeat, u32)>,
    pub(super) payloads: Vec<Payload>,
    pub(super) incoming: Vec<Incoming>,
    pub(super) jobs: Vec<Job>,
    pub(super) next_id: u64,
    pub(super) queue: Clipboard,
}

impl ClipboardState {
    pub fn new(
        globals: &GlobalList,
        qh: &QueueHandle<Platform>,
        seats: impl Iterator<Item = WlSeat>,
        queue: Clipboard,
    ) -> Self {
        let mut state = Self {
            manager: DataDeviceManagerState::bind(globals, qh).ok(),
            devices: Vec::new(),
            serial: None,
            payloads: Vec::new(),
            incoming: Vec::new(),
            jobs: Vec::new(),
            next_id: 0,
            queue,
        };
        for seat in seats {
            state.add_seat(qh, seat);
        }
        state
    }

    pub fn add_seat(&mut self, qh: &QueueHandle<Platform>, seat: WlSeat) {
        if let Some(manager) = &self.manager {
            self.devices.push(SeatTransfers {
                device: manager.get_data_device(qh, &seat),
                seat,
                grab: None,
            });
        }
    }

    pub fn note_serial(&mut self, seat: WlSeat, serial: u32) {
        self.serial = Some((seat, serial));
    }

    pub(crate) fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
}

impl Platform {
    pub(crate) fn shutdown_transfers(&mut self) {
        let jobs: Vec<_> = self.clipboard.jobs.iter().map(|job| job.id).collect();
        for id in jobs {
            self.cancel_transfer(id, Cancellation::AppClosed);
        }
        let payloads = std::mem::take(&mut self.clipboard.payloads);
        for payload in payloads {
            if let Some(drag) = payload.drag {
                self.complete_drag_request(
                    &drag.surface,
                    drag.progress.done,
                    Err(Cancellation::AppClosed.into()),
                );
            }
        }
        let requests: Vec<_> = {
            let mut queue = self.clipboard.queue.0.borrow_mut();
            queue.closed = true;
            queue.requests.drain(..).collect()
        };
        for request in requests {
            super::Queue::reject(request);
        }
        self.clipboard.incoming.clear();
        self.clipboard.devices.clear();
    }

    pub(crate) fn remove_transfer_seat(&mut self, seat: &WlSeat) {
        let jobs: Vec<_> = self
            .clipboard
            .jobs
            .iter()
            .filter(|job| &job.seat == seat)
            .map(|job| job.id)
            .collect();
        for id in jobs {
            self.cancel_transfer(id, Cancellation::InputDeviceGone);
        }
        let devices: Vec<_> = self
            .clipboard
            .devices
            .iter()
            .filter(|device| &device.seat == seat)
            .map(|device| device.device.inner().clone())
            .collect();
        while let Some(index) = self
            .clipboard
            .incoming
            .iter()
            .position(|incoming| devices.contains(&incoming.device))
        {
            let incoming = self.clipboard.incoming.remove(index);
            incoming.offer.destroy();
            self.end_drag_event(
                super::DragEvent::Cancelled {
                    id: incoming.info.id,
                    surface: incoming.info.surface,
                },
                incoming.device.id().protocol_id(),
            );
        }
        for device in self
            .clipboard
            .devices
            .iter()
            .filter(|device| &device.seat == seat)
        {
            if let Some(offer) = device.device.data().selection_offer() {
                offer.destroy();
            }
        }
        self.cancel_drag_sources(seat);
        self.clipboard
            .payloads
            .retain(|payload| &payload.seat != seat);
        self.clipboard.devices.retain(|device| &device.seat != seat);
        if self
            .clipboard
            .serial
            .as_ref()
            .is_some_and(|(current, _)| current == seat)
        {
            self.clipboard.serial = None;
        }
    }

    pub(crate) fn cancel_closed_transfers(&mut self) {
        let jobs: Vec<_> = self
            .clipboard
            .jobs
            .iter()
            .filter(|job| {
                job.surface
                    .as_ref()
                    .is_some_and(|surface| !self.transfer_surface_live(surface))
            })
            .map(|job| job.id)
            .collect();
        for id in jobs {
            self.cancel_transfer(id, Cancellation::SurfaceClosed);
        }
        self.cancel_closed_drag_sources();
    }
}
