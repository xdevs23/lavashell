// SPDX-License-Identifier: GPL-3.0-only
use crate::sctk;
use crate::translator::ViewLink;
use lavashell_platform::{Readiness, WatchEvent, WatchId, WatchRequest};
use sctk::reexports::calloop::generic::Generic;
use sctk::reexports::calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use std::collections::BTreeMap;
use std::os::fd::OwnedFd;

mod error;

pub use error::WatchError;

#[derive(Default)]
pub(crate) struct WatchSources(BTreeMap<WatchId, RegistrationToken>);

impl WatchSources {
    pub(crate) fn execute<D>(
        &mut self,
        handle: &LoopHandle<'static, D>,
        request: WatchRequest,
        view: &ViewLink,
    ) -> Result<(), WatchError> {
        match request {
            WatchRequest::Watch(id, source, readiness) => {
                self.watch(handle, id, source, readiness, view.clone())
            }
            WatchRequest::Resume(id) => {
                if let Some(token) = self.0.get(&id) {
                    handle
                        .enable(token)
                        .map_err(|error| WatchError::ResumeWatch(error.into()))?;
                }
                Ok(())
            }
            WatchRequest::Unwatch(id) => {
                if let Some(token) = self.0.remove(&id) {
                    handle.remove(token);
                }
                Ok(())
            }
        }
    }

    fn watch<D>(
        &mut self,
        handle: &LoopHandle<'static, D>,
        id: WatchId,
        source: OwnedFd,
        readiness: Readiness,
        view: ViewLink,
    ) -> Result<(), WatchError> {
        let interest = match readiness {
            Readiness::Readable => Interest::READ,
            Readiness::Writable => Interest::WRITE,
        };
        let source = Generic::new(source, interest, Mode::Level);
        let token = handle
            .insert_source(source, move |_, _, _| {
                view.report(WatchEvent::Ready(id));
                Ok(PostAction::Disable)
            })
            .map_err(|failure| WatchError::WatchFd(failure.into()))?;
        if let Some(replaced) = self.0.insert(id, token) {
            handle.remove(replaced);
        }
        Ok(())
    }

    pub(crate) fn remove_all<D>(&mut self, handle: &LoopHandle<'static, D>) {
        for token in std::mem::take(&mut self.0).into_values() {
            handle.remove(token);
        }
    }
}

#[cfg(test)]
mod tests;
