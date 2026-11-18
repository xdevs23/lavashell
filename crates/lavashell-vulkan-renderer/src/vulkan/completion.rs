// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell_renderer::{CompletionStatus, FailureList};
use lavashell_vulkan::{Device, Status, Submission};
use std::sync::Arc;

pub struct Completion(Submission);

impl Completion {
    pub(super) fn after_submitted(device: &Arc<Device>) -> Result<Self, Error> {
        device.signal().map(Self).map_err(Error::SubmitCompletion)
    }

    pub(super) fn of_submission(submission: Submission) -> Self {
        Self(submission)
    }

    pub fn status(&self) -> Result<CompletionStatus, Error> {
        match self.0.status().map_err(Error::CompletionStatus)? {
            Status::Pending => Ok(CompletionStatus::Pending),
            Status::Complete => Ok(CompletionStatus::Complete),
        }
    }
}

pub(super) fn finish_completed<T>(
    entries: &mut Vec<T>,
    completion: impl Fn(&T) -> &Completion,
    mut finish: impl FnMut(T),
) -> Result<(), Error> {
    let mut failures = FailureList::default();
    for entry in std::mem::take(entries) {
        match completion(&entry).status() {
            Ok(CompletionStatus::Complete) => finish(entry),
            Ok(CompletionStatus::Pending) => entries.push(entry),
            Err(error) => {
                failures.push(error);
                entries.push(entry);
            }
        }
    }
    failures.into_result()
}
