// SPDX-License-Identifier: GPL-3.0-only

/// What the runtime asks a platform to do with the session lock of the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockRequest {
    /// Asks the host to lock the session. The runtime then creates a lock surface on each output.
    Lock,
    /// Asks the host to end the lock, and reports `UnlockConfirmed` once the host processed it.
    Unlock,
}

/// What the host did with the session lock requested by the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockEvent {
    /// The host locked the session and shows only the lock surfaces.
    Locked,
    /// The host ended the lock, or did not lock the session at all.
    Finished,
    /// The host processed the unlock.
    UnlockConfirmed,
}
