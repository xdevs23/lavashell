// SPDX-License-Identifier: GPL-3.0-only
use super::control::RoleControl;
use super::{NativeRole, SurfaceProtocols};
use crate::bind::bind_global;
use crate::sctk;
use crate::translator::Translator;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{Capability, SessionLockRequest};
use sctk::reexports::client::globals::GlobalList;
use sctk::reexports::client::protocol::{wl_output, wl_surface};
use sctk::reexports::client::{Connection, QueueHandle};
use sctk::reexports::protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1::ExtSessionLockManagerV1,
    ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, ext_session_lock_v1::ExtSessionLockV1,
};

mod error;

pub use error::SessionLockError;

pub(super) type LockSurface = ExtSessionLockSurfaceV1;

pub(crate) struct UnlockSync;

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
    let _sync = connection.display().sync(qh, UnlockSync);
}

pub(super) enum SessionLock {
    NotSupported,
    Available(ExtSessionLockManagerV1),
    Locked(ExtSessionLockV1),
    Spent,
}

impl SessionLock {
    pub(super) fn bind(globals: &GlobalList, qh: &QueueHandle<Translator>) -> Self {
        bind_global(globals, qh).map_or(Self::NotSupported, Self::Available)
    }

    pub(super) fn capability(&self) -> Option<Capability> {
        (!matches!(self, Self::NotSupported)).then_some(Capability::SessionLock)
    }

    fn lock(&mut self, qh: &QueueHandle<Translator>) -> Result<(), SessionLockError> {
        match self {
            Self::NotSupported => Err(SessionLockError::NotSupported),
            Self::Locked(_) | Self::Spent => Err(SessionLockError::AlreadyUsed),
            Self::Available(manager) => {
                *self = Self::Locked(request_lock(manager, qh));
                Ok(())
            }
        }
    }

    fn unlock(
        &mut self,
        connection: &Connection,
        qh: &QueueHandle<Translator>,
    ) -> Result<(), SessionLockError> {
        let Self::Locked(lock) = self else {
            return Err(SessionLockError::UnlockWithoutLock);
        };
        send_unlock(lock, connection, qh);
        *self = Self::Spent;
        Ok(())
    }

    fn lock_surface(
        &self,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
        qh: &QueueHandle<Translator>,
        surface: SurfaceId,
    ) -> Result<LockSurface, SessionLockError> {
        let Self::Locked(lock) = self else {
            return Err(SessionLockError::LockSurfaceWithoutLock(surface));
        };
        Ok(lock.get_lock_surface(wl, output, qh, surface))
    }
}

impl SurfaceProtocols {
    pub(crate) fn lock_role(
        &self,
        wl: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
        qh: &QueueHandle<Translator>,
        surface: SurfaceId,
    ) -> Result<(NativeRole, RoleControl), SessionLockError> {
        let lock_surface = self.lock.lock_surface(wl, output, qh, surface)?;
        Ok((NativeRole::Lock(lock_surface), RoleControl::Stateless))
    }

    pub(crate) fn execute_lock(
        &mut self,
        request: SessionLockRequest,
        connection: &Connection,
        qh: &QueueHandle<Translator>,
    ) -> Result<(), SessionLockError> {
        match request {
            SessionLockRequest::Lock => self.lock.lock(qh),
            SessionLockRequest::Unlock => self.lock.unlock(connection, qh),
        }
    }
}

#[cfg(test)]
mod wire_tests;
