// SPDX-License-Identifier: GPL-3.0-only
mod identity;
#[cfg(test)]
pub(crate) mod test_error;

use std::os::fd::AsFd;

pub(super) use identity::PipeIdentity;

pub(super) fn nonblocking(file: &impl AsFd) -> rustix::io::Result<()> {
    let flags = rustix::fs::fcntl_getfl(file)?;
    rustix::fs::fcntl_setfl(file, flags | rustix::fs::OFlags::NONBLOCK)
}
