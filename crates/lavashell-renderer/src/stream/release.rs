// SPDX-License-Identifier: GPL-3.0-only
use super::{CompletionStatus, Completions, FenceCopyError, ProducedFrame};
use crate::error::FailureList;
use std::os::fd::OwnedFd;
use std::rc::Rc;

enum Work<C> {
    Submitted(Rc<C>),
    Unknown,
}

impl<C> Clone for Work<C> {
    fn clone(&self) -> Self {
        match self {
            Self::Submitted(completion) => Self::Submitted(Rc::clone(completion)),
            Self::Unknown => Self::Unknown,
        }
    }
}

enum ReleaseAfter<C> {
    Fence(Option<OwnedFd>),
    Work(Work<C>),
}

struct Held<C, F> {
    work: Work<C>,
    frame: Option<F>,
}

impl<C, F> Held<C, F> {
    fn finish(self) {
        drop(self.frame);
    }
}

pub(super) struct ReleaseTracker<C: Completions, F> {
    after: ReleaseAfter<C::Completion>,
    held: Vec<Held<C::Completion, F>>,
}

impl<C: Completions, F> Default for ReleaseTracker<C, F> {
    fn default() -> Self {
        Self {
            after: ReleaseAfter::Fence(None),
            held: Vec::new(),
        }
    }
}

impl<C: Completions, F: ProducedFrame> ReleaseTracker<C, F> {
    pub fn wait_for(
        &mut self,
        completions: &C,
        handed_back: Result<Option<OwnedFd>, C::Error>,
    ) -> Result<(), C::Error> {
        let mut failures = FailureList::default();
        let after = match handed_back {
            Ok(fence) => ReleaseAfter::Fence(fence),
            Err(error) => {
                failures.push(error);
                ReleaseAfter::Work(submitted_work(completions, &mut failures))
            }
        };
        if let ReleaseAfter::Work(Work::Submitted(work)) = std::mem::replace(&mut self.after, after)
        {
            self.hold(None, Work::Submitted(work));
        }
        failures.into_result()
    }

    pub fn release(&mut self, completions: &C, frame: F) -> Result<(), C::Error> {
        let copied = match &self.after {
            ReleaseAfter::Fence(fence) => fence.as_ref().map(OwnedFd::try_clone).transpose(),
            ReleaseAfter::Work(work) => {
                let work = work.clone();
                self.hold(Some(frame), work);
                return Ok(());
            }
        };
        match copied {
            Ok(fence) => {
                frame.release(fence);
                Ok(())
            }
            Err(error) => {
                let mut failures = FailureList::default();
                failures.push(C::Error::from(FenceCopyError(error)));
                let work = submitted_work(completions, &mut failures);
                self.hold(Some(frame), work);
                failures.into_result()
            }
        }
    }

    pub fn return_finished(&mut self, completions: &C) -> Result<(), C::Error> {
        let mut failures = FailureList::default();
        for held in std::mem::take(&mut self.held) {
            let status = match &held.work {
                Work::Submitted(completion) => completions.status(completion),
                Work::Unknown => Ok(CompletionStatus::Pending),
            };
            match status {
                Ok(CompletionStatus::Complete) => held.finish(),
                Ok(CompletionStatus::Pending) => self.held.push(held),
                Err(error) => {
                    failures.push(error);
                    self.held.push(held);
                }
            }
        }
        failures.into_result()
    }

    fn hold(&mut self, frame: Option<F>, work: Work<C::Completion>) {
        self.held.push(Held { work, frame });
    }
}

fn submitted_work<C: Completions>(
    completions: &C,
    failures: &mut FailureList<C::Error>,
) -> Work<C::Completion> {
    match completions.completion() {
        Ok(completion) => Work::Submitted(Rc::new(completion)),
        Err(error) => {
            failures.push(error);
            Work::Unknown
        }
    }
}
