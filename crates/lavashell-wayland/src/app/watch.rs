// SPDX-License-Identifier: GPL-3.0-only
use crate::translator::{Translator, ViewLink};
use lavashell_platform::{Readiness, WatchEvent, WatchId, WatchRequest};
use sctk::reexports::calloop::generic::Generic;
use sctk::reexports::calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use smithay_client_toolkit as sctk;
use std::collections::BTreeMap;
use std::os::fd::OwnedFd;

mod error;

pub use error::WatchError;

/// The event loop source of each watched file descriptor.
#[derive(Default)]
pub(crate) struct WatchSources(BTreeMap<WatchId, RegistrationToken>);

impl Translator {
    /// Executes `request` of the runtime on the watched file descriptors.
    pub(crate) fn execute_watch(&mut self, request: WatchRequest) -> Result<(), WatchError> {
        self.watches
            .execute(&self.loop_handle, request, &self.links.view())
    }
}

impl WatchSources {
    /// Executes `request` on the event loop of `handle`. A watch reports through `view`.
    fn execute<D>(
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
                    handle.enable(token).map_err(WatchError::ResumeWatch)?;
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

    /// Watches `source` for `readiness` and reports through `view` once it is ready. The source then pauses until the
    /// runtime handled the report and resumes the watch. Removing the source closes the descriptor.
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
            .map_err(|failure| WatchError::WatchFd(failure.error))?;
        let _no_source_had_the_id = self.0.insert(id, token);
        Ok(())
    }

    /// Removes every watched source from the event loop of `handle`, which closes their descriptors.
    pub(crate) fn remove_all<D>(&mut self, handle: &LoopHandle<'static, D>) {
        for token in std::mem::take(&mut self.0).into_values() {
            handle.remove(token);
        }
    }
}

#[cfg(test)]
mod tests;
