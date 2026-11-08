// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::error::message_chain;
use lavashell_core::view::Failures;

/// Logs each component that failed in a frame as an error that names every cause.
pub(crate) fn log_failures(failures: Failures) {
    for failure in failures {
        log::error!("{}", message_chain(failure));
    }
}

#[cfg(test)]
mod tests;
