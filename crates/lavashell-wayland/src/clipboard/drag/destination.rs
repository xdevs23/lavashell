// SPDX-License-Identifier: GPL-3.0-only
use super::{drag_device, native_action, portable_action, portable_actions};
use crate::clipboard::pipes::{PipeIdentity, nonblocking};
use crate::clipboard::state::Offer;
use crate::translator::Translator;
use lavashell_core::transfer::{DragOfferInfo, DropAcceptance, TransferId};
use lavashell_platform::{DragReceiveRefusal, DragTargetEvent, DragTargetRequest, FormatId};
use sctk::data_device_manager::data_offer::DragOffer;
use sctk::reexports::client::Proxy;
use sctk::reexports::client::protocol::{
    wl_data_device::WlDataDevice, wl_data_device_manager::DndAction, wl_surface::WlSurface,
};
use smithay_client_toolkit as sctk;
use std::os::fd::OwnedFd;

impl Translator {
    /// Reports a drag that entered `surface` on `device`. A drag is refused once the transfer ids are used up.
    pub(crate) fn drag_enter(
        &mut self,
        device: &WlDataDevice,
        surface: &WlSurface,
        x: f64,
        y: f64,
    ) {
        let Some(surface) = self.find_surface(surface) else {
            return;
        };
        let Some(id) = self.clipboard.new_offer_id() else {
            log::warn!("a drag was refused because every transfer id is used up");
            return;
        };
        let Some(current) = self.clipboard.seat_of(device) else {
            return;
        };
        let Some(native) = current.device.data().drag_offer() else {
            return;
        };
        current.entered = Some(id);
        let offer = DragOfferInfo {
            id,
            surface,
            position: crate::logical::point(x, y),
            mime_types: native.with_mime_types(<[String]>::to_vec),
            actions: portable_actions(native.source_actions),
            selected_action: portable_action(native.selected_action),
        };
        let entered = Offer {
            seat: current.seat.clone(),
            native,
            pipe: None,
        };
        let _no_offer_had_the_id = self.clipboard.offers.insert(id, entered);
        self.links.report(DragTargetEvent::Enter {
            device: drag_device(device),
            offer,
        });
    }

    pub(crate) fn drag_motion(&mut self, device: &WlDataDevice, x: f64, y: f64) {
        if let Some(id) = (self.clipboard.seat_of(device)).and_then(|current| current.entered) {
            let at = crate::logical::point(x, y);
            self.links.report(DragTargetEvent::Motion(id, at));
        }
    }

    pub(crate) fn drag_leave(&mut self, device: &WlDataDevice) {
        let Some(id) = self.clipboard.take_entered(device) else {
            return;
        };
        // SCTK destroys the offer of a drag that leaves before a drop. After a drop, the drag is no longer entered.
        let _left = self.clipboard.offers.remove(&id);
        self.links.report(DragTargetEvent::Leave(id));
    }

    pub(crate) fn drag_drop(&mut self, device: &WlDataDevice) {
        if let Some(id) = self.clipboard.take_entered(device) {
            self.links.report(DragTargetEvent::Drop(id));
        }
    }

    pub(crate) fn drag_source_actions(&mut self, offer: &DragOffer) {
        if let Some(id) = self.offer_id(offer) {
            let actions = portable_actions(offer.source_actions);
            self.links
                .report(DragTargetEvent::ActionsChanged(id, actions));
        }
    }

    pub(crate) fn drag_selected_action(&mut self, offer: &DragOffer) {
        if let Some(id) = self.offer_id(offer) {
            let action = portable_action(offer.selected_action);
            self.links
                .report(DragTargetEvent::ActionSelected(id, action));
        }
    }

    /// Executes `request` of the runtime about a drag offered to the app.
    pub(crate) fn execute_drag_target(&mut self, request: DragTargetRequest) {
        match request {
            DragTargetRequest::Answer(id, acceptance) => {
                if let Some(offer) = self.clipboard.offers.get(&id) {
                    answer(&offer.native, acceptance);
                }
            }
            DragTargetRequest::Receive(id, format) => {
                let pipe = self.receive_drop(id, format);
                self.links.report(DragTargetEvent::ReceiveStarted(id, pipe));
            }
            DragTargetRequest::Finish(id) => {
                if let Some(offer) = self.clipboard.offers.remove(&id) {
                    offer.native.finish();
                    offer.native.destroy();
                }
            }
            DragTargetRequest::Release(id) => {
                if let Some(offer) = self.clipboard.offers.remove(&id) {
                    offer.native.destroy();
                }
            }
        }
    }

    /// Requests that the source of the dropped drag `id` write the data in `format` to a new pipe, and returns the
    /// nonblocking read end of that pipe.
    fn receive_drop(
        &mut self,
        id: TransferId,
        format: FormatId,
    ) -> Result<OwnedFd, DragReceiveRefusal> {
        let offer = (self.clipboard.offers.get_mut(&id)).ok_or(DragReceiveRefusal::UnknownOffer)?;
        let pipe = offer
            .native
            .receive(format.0)
            .map_err(DragReceiveRefusal::Pipe)?;
        nonblocking(&pipe).map_err(|errno| DragReceiveRefusal::Pipe(errno.into()))?;
        offer.pipe = PipeIdentity::read(&pipe);
        Ok(pipe.into())
    }

    fn offer_id(&self, offer: &DragOffer) -> Option<TransferId> {
        (self.clipboard.offers.iter())
            .find(|(_, known)| known.native.inner() == offer.inner())
            .map(|(id, _)| *id)
    }
}

/// Accepts a drop of `offer` in the format and with the action of `acceptance`, or rejects the drop for `None`. An
/// offer older than version 3 cannot negotiate the action, so it is rejected.
fn answer(offer: &DragOffer, acceptance: Option<DropAcceptance>) {
    let accepted = acceptance.filter(|_| offer.inner().version() >= 3);
    let action =
        (accepted.as_ref()).map_or(DndAction::empty(), |choice| native_action(choice.action));
    offer.set_actions(action, action);
    offer.accept_mime_type(offer.serial, accepted.map(|choice| choice.mime_type));
}
