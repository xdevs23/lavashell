// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::error::FailureList;
use lavashell_vulkan::{Device, Status, Submission};
use std::sync::Arc;

/// A fence that signals once the GPU work of one submission to a queue is complete. Dropping a `Completion` before
/// that work is complete blocks until the work is complete.
pub struct Completion(Submission);

impl Completion {
    /// A completion that signals once all work submitted to the queue of `device` before this call is complete.
    pub(super) fn after_submitted(device: &Arc<Device>) -> Result<Self, Error> {
        device.signal().map(Self).map_err(Error::SubmitCompletion)
    }

    #[cfg(target_os = "android")]
    pub(super) fn of_submission(submission: Submission) -> Self {
        Self(submission)
    }

    /// # Errors
    /// Fails when the device is lost.
    pub fn status(&self) -> Result<Status, Error> {
        self.0.status().map_err(Error::CompletionStatus)
    }
}

/// Removes every entry of `entries` whose completion has signaled, without waiting for the pending ones. An entry whose
/// status cannot be read is not removed, and its error is returned.
pub(super) fn drop_completed<T>(
    entries: &mut Vec<T>,
    completion: impl Fn(&T) -> &Completion,
) -> Result<(), Error> {
    let mut failures = FailureList::default();
    entries.retain(|entry| match completion(entry).status() {
        Ok(Status::Complete) => false,
        Ok(Status::Pending) => true,
        Err(error) => {
            failures.push(error);
            true
        }
    });
    failures.into_result()
}
