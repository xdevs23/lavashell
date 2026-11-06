// SPDX-License-Identifier: GPL-3.0-only
use super::{Completions, FenceCopyError, ProducedFrame};
use crate::error::FailureList;
use crate::vulkan::Status;
use std::os::fd::OwnedFd;
use std::rc::Rc;

enum Work<C> {
    Submitted(Rc<C>),
    /// The importer failed to submit a completion for the work. The work is known to be finished only at teardown,
    /// once the device is idle.
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
    /// The fence exported after the last render. `None` when the work of that render had already finished.
    Fence(Option<OwnedFd>),
    /// The work of the last render, used after the export of the fence failed.
    Work(Work<C>),
}

/// A frame returned to the producer once the GPU has finished `work`. `_frame` is `None` when only the completion of
/// the work is held. Dropping a completion before the work has finished can block until the GPU finishes.
struct Held<C, F> {
    work: Work<C>,
    _frame: Option<F>,
}

/// Returns released frames to the producers once the GPU has finished every render that could read them.
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
    /// Makes the frames released from now on wait for `handed_back`, the fence returned when the imports were handed
    /// back after a render. When the hand back failed, the frames wait for the GPU work submitted so far.
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

    /// Returns `frame` to the producer once the GPU has finished the last render, which could read the frame.
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
                // Without a copy of the fence, the frame is held until `return_finished` finds the GPU work finished.
                let mut failures = FailureList::default();
                failures.push(C::Error::from(FenceCopyError(error)));
                let work = submitted_work(completions, &mut failures);
                self.hold(Some(frame), work);
                failures.into_result()
            }
        }
    }

    /// Returns the held frames to the producers when the GPU has finished the held work. Frames held for
    /// `Work::Unknown` are returned only at teardown.
    pub fn return_finished(&mut self, completions: &C) -> Result<(), C::Error> {
        let mut failures = FailureList::default();
        self.held.retain(|held| match &held.work {
            Work::Submitted(completion) => match completions.status(completion) {
                Ok(status) => status == Status::Pending,
                Err(error) => {
                    failures.push(error);
                    true
                }
            },
            Work::Unknown => true,
        });
        failures.into_result()
    }

    fn hold(&mut self, frame: Option<F>, work: Work<C::Completion>) {
        self.held.push(Held {
            work,
            _frame: frame,
        });
    }
}

/// The completion of the GPU work submitted so far. When no completion can be submitted for that work, the failure is
/// added to `failures` and the work is `Work::Unknown`.
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
