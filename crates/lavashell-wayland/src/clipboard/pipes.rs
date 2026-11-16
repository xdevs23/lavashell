// SPDX-License-Identifier: GPL-3.0-only
mod identity;
#[cfg(test)]
pub(crate) mod test_error;

use std::io;
use std::os::fd::OwnedFd;

pub(super) use identity::PipeIdentity;

pub(crate) fn into_nonblocking(pipe: impl Into<OwnedFd>) -> io::Result<OwnedFd> {
    let pipe = pipe.into();
    let flags = rustix::fs::fcntl_getfl(&pipe)?;
    rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK)?;
    Ok(pipe)
}
