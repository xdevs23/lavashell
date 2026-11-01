// SPDX-License-Identifier: GPL-3.0-only
use super::{
    DragEvent, DragOfferInfo, DropEvent, Incoming, TransferId, native_action, portable_action,
    portable_actions,
};
use crate::{Error, app::Platform, clipboard::pipes::Scope};
use lavashell_core::transfer::ReceivedData;
use sctk::reexports::client::{
    Proxy,
    protocol::{wl_data_device::WlDataDevice, wl_surface::WlSurface},
};
use smithay_client_toolkit as sctk;

impl Platform {
    pub(crate) fn drag_enter(
        &mut self,
        device: &WlDataDevice,
        surface: &WlSurface,
        x: f64,
        y: f64,
    ) {
        let Some(id) = self.transfer_surface(surface) else {
            return;
        };
        let Some(offer) = self
            .clipboard
            .devices
            .iter()
            .find(|current| current.device.inner() == device)
            .and_then(|current| current.device.data().drag_offer())
        else {
            return;
        };
        let info = DragOfferInfo {
            id: TransferId(self.clipboard.id()),
            surface: id,
            position: crate::logical::point(x, y),
            mime_types: offer.with_mime_types(<[String]>::to_vec),
            actions: portable_actions(offer.source_actions),
            selected_action: portable_action(offer.selected_action),
        };
        self.clipboard.incoming.push(Incoming {
            device: device.clone(),
            offer,
            info: info.clone(),
            acceptance: None,
        });
        self.negotiate_drag(info.id, DragEvent::Enter(info));
    }

    pub(crate) fn drag_motion(&mut self, device: &WlDataDevice, x: f64, y: f64) {
        let Some(incoming) = self
            .clipboard
            .incoming
            .iter_mut()
            .find(|incoming| &incoming.device == device)
        else {
            return;
        };
        incoming.info.position = crate::logical::point(x, y);
        let info = incoming.info.clone();
        self.negotiate_drag(info.id, DragEvent::Motion(info));
    }

    fn negotiate_drag(&mut self, id: TransferId, event: DragEvent) {
        let Some(device) = self
            .clipboard
            .incoming
            .iter()
            .find(|incoming| incoming.info.id == id)
            .map(|incoming| incoming.device.id().protocol_id())
        else {
            return;
        };
        let acceptance = self.dispatch_drag_event(event, device);
        let Some(incoming) = self
            .clipboard
            .incoming
            .iter_mut()
            .find(|incoming| incoming.info.id == id)
        else {
            return;
        };
        incoming.acceptance = acceptance.filter(|choice| {
            incoming.offer.inner().version() >= 3
                && choice.accepts(&incoming.info.mime_types, incoming.info.actions)
        });
        if let Some(choice) = &incoming.acceptance {
            let action = native_action(choice.action);
            incoming.offer.set_actions(action, action);
            incoming
                .offer
                .accept_mime_type(incoming.offer.serial, Some(choice.mime_type.clone()));
        } else {
            incoming.offer.set_actions(
                sctk::reexports::client::protocol::wl_data_device_manager::DndAction::empty(),
                sctk::reexports::client::protocol::wl_data_device_manager::DndAction::empty(),
            );
            incoming.offer.accept_mime_type(incoming.offer.serial, None);
        }
    }

    pub(crate) fn drag_leave(&mut self, device: &WlDataDevice) {
        let Some(index) = self
            .clipboard
            .incoming
            .iter()
            .position(|incoming| &incoming.device == device)
        else {
            return;
        };
        // SCTK destroys the offer of a drag that leaves before a drop. After a drop, `drag_drop` already removed the
        // entry, and the transfer that receives the data destroys the offer.
        let incoming = self.clipboard.incoming.remove(index);
        self.end_drag_event(
            DragEvent::Leave {
                id: incoming.info.id,
                surface: incoming.info.surface,
            },
            incoming.device.id().protocol_id(),
        );
    }

    pub(crate) fn drag_drop(&mut self, device: &WlDataDevice) {
        let Some(index) = self
            .clipboard
            .incoming
            .iter()
            .position(|incoming| &incoming.device == device)
        else {
            return;
        };
        let mut incoming = self.clipboard.incoming.remove(index);
        let native_offer = self
            .clipboard
            .devices
            .iter()
            .find(|current| current.device.inner() == device)
            .and_then(|current| current.device.data().drag_offer());
        if let Some(offer) = native_offer {
            incoming.offer = offer;
        }
        incoming.info.selected_action = portable_action(incoming.offer.selected_action);
        self.end_drag_event(
            DragEvent::Drop(incoming.info.clone()),
            incoming.device.id().protocol_id(),
        );
        let choice = incoming.acceptance.filter(|choice| {
            incoming.info.selected_action == Some(choice.action)
                && choice.accepts(&incoming.info.mime_types, incoming.info.actions)
                && self.transfer_surface_live(&incoming.offer.surface)
        });
        let Some(choice) = choice else {
            self.end_drag_event(
                DragEvent::Cancelled {
                    id: incoming.info.id,
                    surface: incoming.info.surface,
                },
                incoming.device.id().protocol_id(),
            );
            incoming.offer.destroy();
            return;
        };
        let seat = self
            .clipboard
            .devices
            .iter()
            .find(|current| current.device.inner() == device)
            .map(|current| current.seat.clone());
        let Some(seat) = seat else {
            incoming.offer.destroy();
            return;
        };
        let pipe = match incoming.offer.receive(choice.mime_type.clone()) {
            Ok(pipe) => pipe,
            Err(error) => {
                self.complete_drop(
                    &incoming.offer,
                    &incoming.info,
                    choice.action,
                    Err(Error::ReceiveDrag(error)),
                );
                return;
            }
        };
        let surface = incoming.offer.surface.clone();
        self.receive_transfer(
            pipe,
            Scope {
                seat,
                surface: Some(surface),
                source: None,
                incoming: Some(incoming.info.id),
            },
            Box::new(move |platform, bytes| {
                let data = bytes.map(|bytes| ReceivedData {
                    mime_type: choice.mime_type,
                    bytes,
                });
                platform.complete_drop(&incoming.offer, &incoming.info, choice.action, data);
            }),
        );
    }

    fn complete_drop(
        &mut self,
        offer: &sctk::data_device_manager::data_offer::DragOffer,
        info: &DragOfferInfo,
        action: lavashell_core::transfer::DragAction,
        data: Result<ReceivedData, Error>,
    ) {
        let success = data.is_ok() && self.transfer_surface_live(&offer.surface);
        let accepted = self.dispatch_drop_event(DropEvent {
            id: info.id,
            surface: info.surface,
            position: info.position,
            action,
            data,
        });
        if success && accepted {
            offer.finish();
        }
        offer.destroy();
    }

    pub(crate) fn drag_source_actions(
        &mut self,
        offer: &sctk::data_device_manager::data_offer::DragOffer,
    ) {
        if let Some(incoming) = self
            .clipboard
            .incoming
            .iter_mut()
            .find(|incoming| incoming.offer.inner() == offer.inner())
        {
            incoming.info.actions = portable_actions(offer.source_actions);
            let info = incoming.info.clone();
            self.negotiate_drag(info.id, DragEvent::Motion(info));
        }
    }

    pub(crate) fn drag_selected_action(
        &mut self,
        offer: &sctk::data_device_manager::data_offer::DragOffer,
    ) {
        if let Some(incoming) = self
            .clipboard
            .incoming
            .iter_mut()
            .find(|incoming| incoming.offer.inner() == offer.inner())
        {
            incoming.info.selected_action = portable_action(offer.selected_action);
        }
    }
}
