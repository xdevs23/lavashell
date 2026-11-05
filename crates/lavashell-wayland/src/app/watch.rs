// SPDX-License-Identifier: GPL-3.0-only
use super::Platform;
use crate::Error;
use lavashell_platform::{WatchEvent, WatchRequest};
use sctk::reexports::calloop::PostAction;
use sctk::reexports::calloop::generic::Generic;
use sctk::reexports::calloop::{Interest, Mode};
use smithay_client_toolkit as sctk;

impl Platform {
    /// Executes the watch requests made by the runtime. The event loop removes the source of a watch once it reports
    /// the descriptor as readable, and dropping the source closes the descriptor.
    pub(super) fn execute_watch_requests(&mut self) -> Result<(), Error> {
        for request in self.config.action_queue.take_requests() {
            match request {
                WatchRequest::Readable(id, source) => {
                    let source = Generic::new(source, Interest::READ, Mode::Level);
                    let token = (self.loop_handle)
                        .insert_source(source, move |_, _, platform| {
                            let _removed_by_the_event_loop = platform.watches.remove(&id);
                            (platform.config.action_queue).handle(WatchEvent::Readable(id));
                            Ok(PostAction::Remove)
                        })
                        .map_err(|failure| Error::WatchFd(failure.error))?;
                    let _no_source_had_the_id = self.watches.insert(id, token);
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
}
