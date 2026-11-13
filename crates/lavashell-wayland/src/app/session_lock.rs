// SPDX-License-Identifier: GPL-3.0-only
use super::control::RoleControl;
use super::{NativeRole, configure_event};
use crate::translator::Translator;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{LockEvent, LockRequest};
use sctk::reexports::client::protocol::{wl_callback, wl_output, wl_surface};
use sctk::reexports::client::{Connection, Dispatch, QueueHandle, delegate_noop};
use sctk::reexports::protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1::ExtSessionLockManagerV1,
    ext_session_lock_surface_v1::{self, ExtSessionLockSurfaceV1},
    ext_session_lock_v1::{self, ExtSessionLockV1},
};
use smithay_client_toolkit as sctk;
use std::num::NonZeroU32;

mod error;

pub use error::SessionLockError;

pub(super) type LockSurface = ExtSessionLockSurfaceV1;

/// Asks the compositor through `manager` to lock the session. A run locks the session once, so the manager is
/// destroyed.
fn request_lock(
    manager: &ExtSessionLockManagerV1,
    qh: &QueueHandle<Translator>,
) -> ExtSessionLockV1 {
    let proxy = manager.lock(qh, ());
    manager.destroy();
    proxy
}

fn send_unlock(proxy: &ExtSessionLockV1, connection: &Connection, qh: &QueueHandle<Translator>) {
    proxy.unlock_and_destroy();
    // The reply to the sync request shows that the compositor processed the unlock. A compositor that exits before
    // the reply ends the connection instead.
    let _sync = connection.display().sync(qh, UnlockSync);
}

impl Translator {
    /// Gives `wl` the role of the lock surface `surface` of the session lock on `output`.
    pub(super) fn lock_role(
        &self,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
        surface: SurfaceId,
    ) -> Result<(NativeRole, RoleControl), SessionLockError> {
        let lock = (self.session_lock.as_ref())
            .ok_or(SessionLockError::LockSurfaceWithoutLock(surface))?;
        let lock_surface = lock.get_lock_surface(wl, output, &self.qh, surface);
        Ok((NativeRole::Lock(lock_surface), RoleControl::Stateless))
    }

    /// Executes `request` of the runtime on the session lock. The compositor reports what it did as lock events.
    pub(crate) fn execute_lock(&mut self, request: LockRequest) -> Result<(), SessionLockError> {
        match request {
            LockRequest::Lock => {
                let manager =
                    (self.protocols.lock.take()).ok_or(SessionLockError::NoSessionLock)?;
                self.session_lock = Some(request_lock(&manager, &self.qh));
            }
            LockRequest::Unlock => {
                let lock = (self.session_lock.take()).ok_or(SessionLockError::UnlockWithoutLock)?;
                send_unlock(&lock, &self.connection, &self.qh);
            }
        }
        Ok(())
    }
}

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
            ext_session_lock_v1::Event::Locked => LockEvent::Locked,
            ext_session_lock_v1::Event::Finished => LockEvent::Finished,
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
            // A lock surface cannot be drawn at a size of zero. Such a configure is not acknowledged, and the
            // surface continues with the size of the last configure.
            let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height))
            else {
                log::warn!(
                    "lock surface {} was configured with the empty size {width}x{height}",
                    id.0
                );
                return;
            };
            surface.ack_configure(serial);
            state
                .links
                .report((*id, configure_event(Some(width), Some(height), None)));
        }
    }
}

pub(super) struct UnlockSync;

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
            state.links.report(LockEvent::UnlockConfirmed);
        }
    }
}

#[cfg(test)]
mod wire_tests;
