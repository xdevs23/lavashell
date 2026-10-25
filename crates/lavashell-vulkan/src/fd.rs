// SPDX-License-Identifier: GPL-3.0-only
use std::os::fd::{FromRawFd, OwnedFd, RawFd};

/// Wraps a file descriptor exported by Vulkan in an `OwnedFd`, which closes the descriptor on drop. Returns `None`
/// for -1, the value returned by a sync fd export when the work that the fd would wait for is already complete.
///
/// # Safety
/// `raw` must be the result of a Vulkan fd export, owned by nobody else.
pub(crate) unsafe fn exported_fd(raw: RawFd) -> Option<OwnedFd> {
    // SAFETY: std, rustix and nix offer no safe way to adopt a raw descriptor, so `from_raw_fd` is the only form.
    // It needs an open descriptor owned by nobody else: a non-negative export result is open, and the caller gives
    // up its sole ownership.
    (raw >= 0).then(|| unsafe { OwnedFd::from_raw_fd(raw) })
}
