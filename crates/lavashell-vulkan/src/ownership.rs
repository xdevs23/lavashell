// SPDX-License-Identifier: GPL-3.0-only
use std::any::Any;
use std::sync::Arc;

/// A Vulkan handle that this crate created and destroys, or a handle created and destroyed by a host engine. A
/// borrowed handle is valid as long as `_keep_alive` exists, which the host promises when it lends the handle.
pub(crate) enum Ownership<T> {
    Owned(T),
    Borrowed {
        handle: T,
        _keep_alive: Arc<dyn Any + Send + Sync>,
    },
}

impl<T> Ownership<T> {
    pub(crate) fn handle(&self) -> &T {
        match self {
            Self::Owned(handle) | Self::Borrowed { handle, .. } => handle,
        }
    }
}
