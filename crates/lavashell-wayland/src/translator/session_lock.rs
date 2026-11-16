// SPDX-License-Identifier: GPL-3.0-only
use super::Translator;
use crate::native::{UnlockSync, configure_event};
use crate::sctk;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::SessionLockEvent;
use sctk::reexports::client::protocol::wl_callback;
use sctk::reexports::client::{Connection, Dispatch, QueueHandle, delegate_noop};
use sctk::reexports::protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1::ExtSessionLockManagerV1,
    ext_session_lock_surface_v1::{self, ExtSessionLockSurfaceV1},
    ext_session_lock_v1::{self, ExtSessionLockV1},
};
use std::num::NonZeroU32;

delegate_noop!(Translator: ignore ExtSessionLockManagerV1);

impl Dispatch<ExtSessionLockV1, ()> for Translator {
    fn event(
        state: &mut Self,
        _: &ExtSessionLockV1,
        event: ext_session_lock_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let event = match event {
            ext_session_lock_v1::Event::Locked => SessionLockEvent::Locked,
            ext_session_lock_v1::Event::Finished => SessionLockEvent::Finished,
            _ => return,
        };
        state.links.report(event);
    }
}

impl Dispatch<ExtSessionLockSurfaceV1, SurfaceId> for Translator {
    fn event(
        state: &mut Self,
        surface: &ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        id: &SurfaceId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_session_lock_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
            && state.surfaces.is_live(*id)
        {
            let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height))
            else {
                log::warn!(
                    "lock surface {} was configured with the empty size {width}x{height}",
                    id.get()
                );
                return;
            };
            surface.ack_configure(serial);
            state
                .links
                .report((*id, configure_event(Some(width), Some(height), None, None)));
        }
    }
}

impl Dispatch<wl_callback::WlCallback, UnlockSync> for Translator {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &UnlockSync,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.links.report(SessionLockEvent::UnlockConfirmed);
        }
    }
}
