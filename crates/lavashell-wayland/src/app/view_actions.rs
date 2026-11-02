// SPDX-License-Identifier: GPL-3.0-only
use super::Platform;
use crate::Error;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use sctk::reexports::calloop::PostAction;
use sctk::reexports::calloop::generic::Generic;
use sctk::reexports::calloop::{Interest, Mode, RegistrationToken};
use smithay_client_toolkit as sctk;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

type Action = Box<dyn FnOnce(&mut View)>;

pub(super) struct PendingAction {
    pub surface: SurfaceId,
    pub apply: Action,
}

/// An action that waits until its file descriptor becomes readable.
struct ReadableWatch {
    source: OwnedFd,
    action: PendingAction,
}

/// The registration of an event loop source that waits until the file descriptor of a view action becomes
/// readable. `fd` identifies the source among the waiting ones, because the source owns the descriptor and no
/// other open source can have the same number.
pub(super) struct ReadableSource {
    fd: RawFd,
    pub token: RegistrationToken,
}

#[derive(Default)]
struct Queue {
    pending: VecDeque<PendingAction>,
    watches: Vec<ReadableWatch>,
    closed: bool,
}

/// A queue of actions on the view of one surface each, filled by controls and by asynchronous transfer
/// callbacks. The event loop runs each action on the UI thread after it composes the view of that surface. An
/// action that requests a transfer captures the text it needs when it is queued, because the focus may have moved
/// by the time the data arrives.
#[derive(Clone, Default)]
pub struct ViewActions(Rc<RefCell<Queue>>);

impl ViewActions {
    /// Queues `apply` for the view of `surface`. The action is dropped when that surface closes or is retired
    /// before the action runs.
    pub fn dispatch(&self, surface: SurfaceId, apply: impl FnOnce(&mut View) + 'static) {
        self.queue(PendingAction {
            surface,
            apply: Box::new(apply),
        });
    }

    /// Queues `apply` once `source` becomes readable, such as the pidfd of a child process when
    /// it exits. `source` is closed after that.
    pub fn dispatch_when_readable(
        &self,
        source: OwnedFd,
        surface: SurfaceId,
        apply: impl FnOnce(&mut View) + 'static,
    ) {
        let mut queue = self.0.borrow_mut();
        if !queue.closed {
            queue.watches.push(ReadableWatch {
                source,
                action: PendingAction {
                    surface,
                    apply: Box::new(apply),
                },
            });
        }
    }

    fn queue(&self, action: PendingAction) {
        let mut queue = self.0.borrow_mut();
        if !queue.closed {
            queue.pending.push_back(action);
        }
    }

    pub(super) fn pending(&self) -> bool {
        !self.0.borrow().pending.is_empty()
    }

    pub(super) fn restore(&self, pending: Vec<PendingAction>) {
        let mut queue = self.0.borrow_mut();
        for action in pending.into_iter().rev() {
            queue.pending.push_front(action);
        }
    }

    pub(super) fn drain(&self) -> Vec<PendingAction> {
        self.0.borrow_mut().pending.drain(..).collect()
    }

    fn take_watches(&self) -> Vec<ReadableWatch> {
        std::mem::take(&mut self.0.borrow_mut().watches)
    }

    pub(super) fn close(&self) {
        let (pending, watches) = {
            let mut queue = self.0.borrow_mut();
            queue.closed = true;
            (
                std::mem::take(&mut queue.pending),
                std::mem::take(&mut queue.watches),
            )
        };
        drop((pending, watches));
    }
}

impl Platform {
    /// Adds the sources of the actions that wait for readable file descriptors to the event loop.
    /// A source removes its entry from `readable_sources` when it fires, because the event loop removes the source
    /// at that point.
    pub(super) fn watch_readable_sources(&mut self) -> Result<(), Error> {
        for watch in self.config.view_actions.take_watches() {
            let mut action = Some(watch.action);
            let fd = watch.source.as_raw_fd();
            let source = Generic::new(watch.source, Interest::READ, Mode::Level);
            let token = (self.loop_handle)
                .insert_source(source, move |_, _, platform| {
                    platform.readable_sources.retain(|waiting| waiting.fd != fd);
                    if let Some(action) = action.take() {
                        platform.config.view_actions.queue(action);
                    }
                    Ok(PostAction::Remove)
                })
                .map_err(|failure| Error::WatchFd(failure.error))?;
            self.readable_sources.push(ReadableSource { fd, token });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_before_callbacks_and_closes_without_reentrant_borrows() {
        let queue = ViewActions::default();
        let again = queue.clone();
        queue.dispatch(SurfaceId(3), move |_| again.dispatch(SurfaceId(4), |_| {}));
        let pending = queue.drain();
        assert!(!queue.pending());
        let mut view = View::new(lavashell_test_fixtures::facts(), |_| {});
        assert_eq!(pending[0].surface, SurfaceId(3));
        for action in pending {
            (action.apply)(&mut view);
        }
        assert_eq!(queue.drain()[0].surface, SurfaceId(4));
        queue.close();
        queue.dispatch(SurfaceId(3), |_| {});
        assert!(!queue.pending());
    }
}
