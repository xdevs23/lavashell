// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;
use sctk::reexports::client::globals::BindError;
use smithay_client_toolkit as sctk;

/// A failure of the session lock. Each variant is one operation that can fail.
#[derive(Debug, thiserror::Error)]
pub enum SessionLockError {
    #[error("ext-session-lock-v1 could not be bound")]
    BindSessionLock(#[source] BindError),
    #[error("the runtime requested the lock surface {0:?} while the session is not locked")]
    LockSurfaceWithoutLock(SurfaceId),
    #[error("the runtime requested an unlock while the session is not locked")]
    UnlockWithoutLock,
}
