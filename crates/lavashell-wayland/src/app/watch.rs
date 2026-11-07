// SPDX-License-Identifier: GPL-3.0-only
use super::Platform;
use crate::Error;
use lavashell_platform::{Readiness, WatchEvent, WatchId, WatchRequest};
use sctk::reexports::calloop::PostAction;
use sctk::reexports::calloop::generic::Generic;
use sctk::reexports::calloop::{Interest, Mode};
use smithay_client_toolkit as sctk;
use std::os::fd::OwnedFd;

impl Platform {
    /// Executes the watch requests made by the runtime.
    pub(super) fn execute_watch_requests(&mut self) -> Result<(), Error> {
        for request in self.config.watches.take_requests() {
            match request {
                WatchRequest::Watch(id, source, readiness) => {
                    self.watch_descriptor(id, source, readiness)?;
                }
                WatchRequest::Unwatch(id) => {
                    if let Some(token) = self.watches.remove(&id) {
                        self.loop_handle.remove(token);
                    }
                }
            }
        }
        Ok(())
    }

    /// Watches `source` for `readiness` and reports each time it is ready. `Unwatch` removes the source of the watch,
    /// and dropping the source closes the descriptor.
    fn watch_descriptor(
        &mut self,
        id: WatchId,
        source: OwnedFd,
        readiness: Readiness,
    ) -> Result<(), Error> {
        let interest = match readiness {
            Readiness::Readable => Interest::READ,
            Readiness::Writable => Interest::WRITE,
        };
        let source = Generic::new(source, interest, Mode::Level);
        let token = (self.loop_handle)
            .insert_source(source, move |_, _, platform| {
                platform.config.watches.handle(WatchEvent::Ready(id));
                Ok(PostAction::Continue)
            })
            .map_err(|failure| Error::WatchFd(failure.error))?;
        let _no_source_had_the_id = self.watches.insert(id, token);
        Ok(())
    }
}
