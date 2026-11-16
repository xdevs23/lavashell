// SPDX-License-Identifier: GPL-3.0-only
use std::os::fd::AsFd;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::clipboard) struct PipeIdentity {
    device: u64,
    inode: u64,
}

impl PipeIdentity {
    pub fn read(file: &impl AsFd) -> Option<Self> {
        let stat = rustix::fs::fstat(file).ok()?;
        (rustix::fs::FileType::from_raw_mode(stat.st_mode) == rustix::fs::FileType::Fifo).then_some(
            Self {
                device: stat.st_dev,
                inode: stat.st_ino,
            },
        )
    }
}

#[cfg(test)]
mod tests;
