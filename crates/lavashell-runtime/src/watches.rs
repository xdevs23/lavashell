// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::transfer::Cancellation;
use lavashell_platform::{Readiness, WatchEvent, WatchId, WatchRequest};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::os::fd::OwnedFd;
use std::rc::Rc;

mod error;

pub use error::Error;

/// Whether a watch goes on after its descriptor was reported as ready.
pub(crate) enum AfterReady {
    Keep,
    End,
}

type OnReady = Box<dyn FnMut(WatchId) -> AfterReady>;

#[derive(Default)]
struct Registry {
    next_watch_id: u64,
    watches: BTreeMap<WatchId, OnReady>,
    requests: Vec<WatchRequest>,
    closed: bool,
}

/// The file descriptors that the runtime asks a platform to watch, and what runs each time one of them is ready. Every
/// watch of a run takes its id from here.
#[derive(Clone, Default)]
pub struct Watches(Rc<RefCell<Registry>>);

impl Watches {
    /// Asks the platform to watch `descriptor` for `readiness`, and runs `on_ready` each time the descriptor is ready
    /// until `on_ready` returns `AfterReady::End` or the watch is ended.
    pub(crate) fn watch(
        &self,
        descriptor: OwnedFd,
        readiness: Readiness,
        on_ready: impl FnMut(WatchId) -> AfterReady + 'static,
    ) -> Result<WatchId, Error> {
        let mut registry = self.0.borrow_mut();
        if registry.closed {
            return Err(Cancellation::AppClosed.into());
        }
        let id = WatchId(registry.next_watch_id);
        registry.next_watch_id = id.0.checked_add(1).ok_or(Error::NumberWatch)?;
        registry
            .requests
            .push(WatchRequest::Watch(id, descriptor, readiness));
        let _no_watch_had_the_id = registry.watches.insert(id, Box::new(on_ready));
        Ok(id)
    }

    /// Ends the watch `id` and requests that the platform close its descriptor, unless the watch ended already.
    pub(crate) fn unwatch(&self, id: WatchId) {
        let mut registry = self.0.borrow_mut();
        let on_ready = registry.watches.remove(&id);
        if on_ready.is_some() {
            registry.requests.push(WatchRequest::Unwatch(id));
        }
        drop(registry);
        drop(on_ready);
    }

    /// Takes the watch requests made since the last call, for the platform to execute.
    #[must_use]
    pub fn take_requests(&self) -> Vec<WatchRequest> {
        std::mem::take(&mut self.0.borrow_mut().requests)
    }

    /// Runs what waits for the watch that the platform reported as ready. Asks the platform to report the watch again
    /// when it goes on, or ends the watch when that asks for it.
    pub fn handle(&self, event: WatchEvent) {
        let WatchEvent::Ready(id) = event;
        let on_ready = self.0.borrow_mut().watches.remove(&id);
        let Some(mut on_ready) = on_ready else {
            return;
        };
        let after = on_ready(id);
        let mut registry = self.0.borrow_mut();
        if registry.closed {
            return;
        }
        match after {
            AfterReady::Keep => {
                let _no_watch_had_the_id = registry.watches.insert(id, on_ready);
                registry.requests.push(WatchRequest::Resume(id));
            }
            AfterReady::End => registry.requests.push(WatchRequest::Unwatch(id)),
        }
    }

    /// Drops every watch and request, and refuses every watch asked for after that.
    pub fn close(&self) {
        let mut registry = self.0.borrow_mut();
        registry.closed = true;
        let dropped = (
            std::mem::take(&mut registry.watches),
            std::mem::take(&mut registry.requests),
        );
        drop(registry);
        drop(dropped);
    }
}
