// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct EventLoopFailure(calloop::Error);

impl From<calloop::Error> for EventLoopFailure {
    fn from(error: calloop::Error) -> Self {
        Self(error)
    }
}

impl<T> From<calloop::InsertError<T>> for EventLoopFailure {
    fn from(failure: calloop::InsertError<T>) -> Self {
        Self(failure.error)
    }
}

impl fmt::Display for EventLoopFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.0 {
            calloop::Error::InvalidToken => "the event source token is invalid",
            calloop::Error::IoError(_) => "an input or output operation failed",
            calloop::Error::OtherError(_) => "an event source failed",
        })
    }
}

impl Error for EventLoopFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}

#[cfg(test)]
mod tests;
