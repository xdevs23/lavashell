// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionLockRequest {
    Lock,
    Unlock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionLockEvent {
    Locked,
    Finished,
    UnlockConfirmed,
}
