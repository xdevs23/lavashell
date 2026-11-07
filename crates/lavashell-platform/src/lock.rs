// SPDX-License-Identifier: GPL-3.0-only

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
