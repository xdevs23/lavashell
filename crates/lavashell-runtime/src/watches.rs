// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::id_counter::IdCounter;
use lavashell_core::transfer::Cancellation;
use lavashell_platform::{Readiness, WatchEvent, WatchId, WatchRequest};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::os::fd::OwnedFd;
use std::rc::Rc;

mod error;

pub use error::Error;

pub(crate) enum AfterReady {
    Keep,
    End,
}

type OnReady = Box<dyn FnMut(WatchId) -> AfterReady>;

#[derive(Default)]
struct Registry {
    watch_ids: IdCounter,
    watches: BTreeMap<WatchId, OnReady>,
    requests: Vec<WatchRequest>,
    closed: bool,
}

#[derive(Clone, Default)]
pub struct Watches(Rc<RefCell<Registry>>);

impl Watches {
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
        let id = WatchId(registry.watch_ids.next_id().map_err(Error::NumberWatch)?);
        registry
            .requests
            .push(WatchRequest::Watch(id, descriptor, readiness));
        let _no_watch_had_the_id = registry.watches.insert(id, Box::new(on_ready));
        Ok(id)
    }

    pub(crate) fn unwatch(&self, id: WatchId) {
        let mut registry = self.0.borrow_mut();
        let on_ready = registry.watches.remove(&id);
        if on_ready.is_some() {
            registry.requests.push(WatchRequest::Unwatch(id));
        }
        drop(registry);
        drop(on_ready);
    }

    #[must_use]
    pub fn take_requests(&self) -> Vec<WatchRequest> {
        std::mem::take(&mut self.0.borrow_mut().requests)
    }

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
