// SPDX-License-Identifier: GPL-3.0-only
use super::Translator;
use crate::sctk;
use lavashell_platform::{DragTargetEvent, DragTargetRequest};
use sctk::data_device_manager::data_device::DataDeviceHandler;
use sctk::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use sctk::reexports::client::protocol::{
    wl_data_device::WlDataDevice, wl_data_device_manager::DndAction, wl_surface::WlSurface,
};
use sctk::reexports::client::{Connection, QueueHandle};

impl Translator {
    pub(super) fn execute_drag_target(&mut self, request: DragTargetRequest) {
        match request {
            DragTargetRequest::Answer(id, acceptance) => {
                self.clipboard.answer_offer(id, acceptance);
            }
            DragTargetRequest::Receive(id, format) => {
                let pipe = self.clipboard.receive_drop(id, format);
                self.links.report(DragTargetEvent::ReceiveStarted(id, pipe));
            }
            DragTargetRequest::Finish(id) => self.clipboard.finish_offer(id),
            DragTargetRequest::Release(id) => self.clipboard.release_offer(id),
        }
    }

    fn report_drag_target(&self, event: Option<DragTargetEvent>) {
        if let Some(event) = event {
            self.links.report(event);
        }
    }
}

impl DataDeviceHandler for Translator {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        device: &WlDataDevice,
        x: f64,
        y: f64,
        surface: &WlSurface,
    ) {
        let Some(surface) = self.find_surface(surface) else {
            return;
        };
        let position = crate::logical::convert_to_point(x, y);
        let entered = self.clipboard.enter_drag(device, surface, position);
        self.report_drag_target(entered);
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        let left = self.clipboard.leave_drag(device);
        self.report_drag_target(left);
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        device: &WlDataDevice,
        x: f64,
        y: f64,
    ) {
        let position = crate::logical::convert_to_point(x, y);
        let moved = self.clipboard.move_drag(device, position);
        self.report_drag_target(moved);
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        self.links.report(self.clipboard.selection_offered());
    }

    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        let dropped = self.clipboard.drop_drag(device);
        self.report_drag_target(dropped);
    }
}

impl DataOfferHandler for Translator {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        offer: &mut DragOffer,
        _: DndAction,
    ) {
        let changed = self.clipboard.offer_actions_changed(offer);
        self.report_drag_target(changed);
    }

    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        offer: &mut DragOffer,
        _: DndAction,
    ) {
        let selected = self.clipboard.offer_action_selected(offer);
        self.report_drag_target(selected);
    }
}
