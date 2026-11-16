// SPDX-License-Identifier: GPL-3.0-only
use crate::error::ProcessError;
use rustix::event::{PollFd, PollFlags, poll};
use rustix::io::Errno;
use std::os::fd::BorrowedFd;

pub fn first_ready<Source: Copy>(
    sources: &[(BorrowedFd<'_>, Source)],
) -> Result<Source, ProcessError> {
    let mut watched: Vec<PollFd<'_>> = (sources.iter())
        .map(|&(descriptor, _)| PollFd::from_borrowed_fd(descriptor, PollFlags::IN))
        .collect();
    loop {
        match poll(&mut watched, None) {
            Ok(_ready_count) => {}
            Err(Errno::INTR) => {}
            Err(error) => return Err(ProcessError::WaitForReadiness(error)),
        }
        let ready = (watched.iter().zip(sources))
            .find(|(descriptor, _)| !descriptor.revents().is_empty())
            .map(|(_, &(_, source))| source);
        if let Some(source) = ready {
            return Ok(source);
        }
    }
}
