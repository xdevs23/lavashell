// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::error::message_chain;
use lavashell_core::view::Failures;
use std::error::Error;

pub(crate) fn log_failures(failures: Failures) {
    for failure in failures {
        log::error!("{}", message_chain(failure));
    }
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
