// SPDX-License-Identifier: GPL-3.0-only
use super::Platform;
use crate::Error;
use lavashell_actor::Closed;
use sctk::reexports::calloop::channel::{self, Event};
use sctk::reexports::calloop::{self, LoopHandle, RegistrationToken};
use smithay_client_toolkit as sctk;

/// Watches the messages of an actor in the event loop, and returns the observer that sends each message there.
/// The event loop passes each message to `on_message`, and calls `on_closed` once the actor dropped the observer.
pub(super) fn watch<T: Send + 'static>(
    handle: &LoopHandle<'static, Platform>,
    sources: &mut Vec<RegistrationToken>,
    on_message: fn(&mut Platform, T),
    on_closed: fn(&mut Platform),
    watch_error: fn(calloop::Error) -> Error,
) -> Result<impl FnMut(T) -> Result<(), Closed> + Send + 'static, Error> {
    let (sender, channel) = channel::channel();
    let source = handle
        .insert_source(channel, move |event, (), platform| match event {
            Event::Msg(message) => on_message(platform, message),
            Event::Closed => on_closed(platform),
        })
        .map_err(|failure| watch_error(failure.error))?;
    sources.push(source);
    Ok(move |message: T| sender.send(message).map_err(|_unsent| Closed))
}

/// The error of a stopped actor: the failure returned by `stop`, or `clean` when the actor did not fail.
pub(super) fn stopped(stop: Result<(), lavashell_runtime::Error>, clean: Error) -> Error {
    stop.map_or_else(Error::Runtime, |()| clean)
}
