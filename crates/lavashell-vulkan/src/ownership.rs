// SPDX-License-Identifier: GPL-3.0-only
use std::any::Any;
use std::sync::Arc;

pub type KeepAlive = Arc<dyn Any + Send + Sync>;

pub(crate) enum Ownership<T> {
    Owned(T),
    Borrowed { handle: T, _keep_alive: KeepAlive },
}

impl<T> Ownership<T> {
    pub(crate) fn handle(&self) -> &T {
        match self {
            Self::Owned(handle) | Self::Borrowed { handle, .. } => handle,
        }
    }
}
