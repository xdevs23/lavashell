// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;

#[derive(Debug, thiserror::Error)]
pub enum SessionLockError {
    #[error("the compositor does not support ext-session-lock-v1")]
    NotSupported,
    #[error(
        "the runtime requested a lock after the session lock of this run was already requested"
    )]
    AlreadyUsed,
    #[error("the runtime requested the lock surface {0:?} while the session is not locked")]
    LockSurfaceWithoutLock(SurfaceId),
    #[error("the runtime requested an unlock while the session is not locked")]
    UnlockWithoutLock,
}
