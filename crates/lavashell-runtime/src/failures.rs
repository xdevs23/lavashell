// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::view::ComponentFailure;
use lavashell_error_chain::message_chain;
use std::error::Error;

pub(crate) fn log_failure(failure: &ComponentFailure) {
    log::error!("{}", message_chain(failure));
}

pub(crate) fn log_failure_to_stop(stopped: Result<(), impl Error>) {
    if let Err(failure) = stopped {
        log::error!("{}", message_chain(failure));
    }
}

#[cfg(test)]
pub(crate) mod test_error;
#[cfg(test)]
mod tests;
