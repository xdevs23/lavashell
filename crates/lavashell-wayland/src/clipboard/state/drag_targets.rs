// SPDX-License-Identifier: GPL-3.0-only
use super::{ClipboardState, DragTargetOffer};
use crate::clipboard::drag::{drag_device, native_action, portable_action, portable_actions};
use crate::clipboard::pipes::{PipeIdentity, into_nonblocking};
use crate::sctk;
use lavashell_core::geometry::Point;
use lavashell_core::id_counter::IdsExhausted;
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{DragOfferInfo, DropAcceptance, TransferId};
use lavashell_platform::{DragReceiveRefusal, DragTargetEvent, FormatId};
use sctk::data_device_manager::data_offer::DragOffer;
use sctk::reexports::client::Proxy;
use sctk::reexports::client::protocol::{
    wl_data_device::WlDataDevice, wl_data_device_manager::DndAction, wl_data_offer, wl_seat::WlSeat,
};
use std::os::fd::OwnedFd;

impl ClipboardState {
    pub(crate) fn enter_drag(
        &mut self,
        device: &WlDataDevice,
        surface: SurfaceId,
        position: Point,
    ) -> Option<DragTargetEvent> {
        let Ok(id) = self.new_offer_id() else {
            log::warn!("a drag was refused because every transfer id is used up");
            return None;
        };
        let current = self.seat_of(device)?;
        let native = current.device.data().drag_offer()?;
        current.entered = Some(id);
        let offer = DragOfferInfo {
            id,
            surface,
            position,
            mime_types: native.with_mime_types(<[String]>::to_vec),
            actions: portable_actions(native.source_actions),
            selected_action: portable_action(native.selected_action),
        };
        let entered = DragTargetOffer {
            seat: current.seat.clone(),
            native,
            pipe: None,
        };
        let _no_offer_had_the_id = self.offers.insert(id, entered);
        Some(DragTargetEvent::Enter {
            device: drag_device(device),
            offer,
        })
    }

    pub(crate) fn move_drag(
        &mut self,
        device: &WlDataDevice,
        position: Point,
    ) -> Option<DragTargetEvent> {
        let id = self.seat_of(device)?.entered?;
        Some(DragTargetEvent::Motion(id, position))
    }

    pub(crate) fn leave_drag(&mut self, device: &WlDataDevice) -> Option<DragTargetEvent> {
        let id = self.take_entered(device)?;
        let _left = self.offers.remove(&id);
        Some(DragTargetEvent::Leave(id))
    }

    pub(crate) fn drop_drag(&mut self, device: &WlDataDevice) -> Option<DragTargetEvent> {
        self.take_entered(device).map(DragTargetEvent::Drop)
    }

    pub(crate) fn offer_actions_changed(&self, offer: &DragOffer) -> Option<DragTargetEvent> {
        let id = self.offer_id(offer)?;
        Some(DragTargetEvent::ActionsChanged(
            id,
            portable_actions(offer.source_actions),
        ))
    }

    pub(crate) fn offer_action_selected(&self, offer: &DragOffer) -> Option<DragTargetEvent> {
        let id = self.offer_id(offer)?;
        Some(DragTargetEvent::ActionSelected(
            id,
            portable_action(offer.selected_action),
        ))
    }

    pub(crate) fn answer_offer(&self, id: TransferId, acceptance: Option<DropAcceptance>) {
        if let Some(offer) = self.offers.get(&id) {
            answer(&offer.native, acceptance);
        }
    }

    pub(crate) fn finish_offer(&mut self, id: TransferId) {
        if let Some(offer) = self.offers.remove(&id) {
            offer.native.finish();
            offer.native.destroy();
        }
    }

    pub(crate) fn release_offer(&mut self, id: TransferId) {
        if let Some(offer) = self.offers.remove(&id) {
            offer.native.destroy();
        }
    }

    pub(crate) fn receive_drop(
        &mut self,
        id: TransferId,
        format: FormatId,
    ) -> Result<OwnedFd, DragReceiveRefusal> {
        let offer = self
            .offers
            .get_mut(&id)
            .ok_or(DragReceiveRefusal::UnknownOffer)?;
        let pipe = offer
            .native
            .receive(format.0)
            .map_err(DragReceiveRefusal::Pipe)?;
        let pipe = into_nonblocking(pipe).map_err(DragReceiveRefusal::Pipe)?;
        offer.pipe = PipeIdentity::read(&pipe);
        Ok(pipe)
    }

    pub(super) fn cancel_offers_of(&mut self, seat: &WlSeat) -> Vec<DragTargetEvent> {
        let cancelled: Vec<_> = (self.offers)
            .extract_if(.., |_, offer| &offer.seat == seat)
            .collect();
        (cancelled.into_iter())
            .map(|(id, offer)| {
                offer.native.destroy();
                DragTargetEvent::Cancelled(id)
            })
            .collect()
    }

    fn new_offer_id(&mut self) -> Result<TransferId, IdsExhausted> {
        self.offer_ids.next_id().map(TransferId)
    }

    fn seat_of(&mut self, device: &WlDataDevice) -> Option<&mut super::SeatTransfers> {
        (self.devices.iter_mut()).find(|current| current.device.inner() == device)
    }

    fn take_entered(&mut self, device: &WlDataDevice) -> Option<TransferId> {
        self.seat_of(device)?.entered.take()
    }

    fn offer_id(&self, offer: &DragOffer) -> Option<TransferId> {
        (self.offers.iter())
            .find(|(_, known)| known.native.inner() == offer.inner())
            .map(|(id, _)| *id)
    }
}

fn answer(offer: &DragOffer, acceptance: Option<DropAcceptance>) {
    let accepted =
        acceptance.filter(|_| offer.inner().version() >= wl_data_offer::REQ_SET_ACTIONS_SINCE);
    let action =
        (accepted.as_ref()).map_or(DndAction::empty(), |choice| native_action(choice.action));
    offer.set_actions(action, action);
    offer.accept_mime_type(offer.serial, accepted.map(|choice| choice.mime_type));
}
