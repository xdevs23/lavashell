// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;

/// A failure of the session lock. Each variant is one operation that can fail.
#[derive(Debug, thiserror::Error)]
pub enum SessionLockError {
    #[error(
        "the compositor does not support ext-session-lock-v1, or an earlier lock of the run used it"
    )]
    NoSessionLock,
    #[error("the runtime requested the lock surface {0:?} while the session is not locked")]
    LockSurfaceWithoutLock(SurfaceId),
    #[error("the runtime requested an unlock while the session is not locked")]
    UnlockWithoutLock,
}
