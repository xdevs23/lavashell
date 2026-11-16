// SPDX-License-Identifier: GPL-3.0-only
use crate::sctk;
use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::{Dispatch, Proxy, QueueHandle};

pub(crate) fn bind_optional<T>(protocol: &str, bound: Result<T, BindError>) -> Option<T> {
    match bound {
        Ok(global) => Some(global),
        Err(BindError::NotPresent) => None,
        Err(error @ BindError::UnsupportedVersion) => {
            log::warn!(
                "the compositor offers {protocol} in a version this program cannot use: {error}"
            );
            None
        }
    }
}

pub(crate) fn bind_global<I, State>(globals: &GlobalList, qh: &QueueHandle<State>) -> Option<I>
where
    I: Proxy + 'static,
    State: Dispatch<I, ()> + 'static,
{
    let interface = I::interface();
    bind_optional(interface.name, globals.bind(qh, 1..=interface.version, ()))
}
