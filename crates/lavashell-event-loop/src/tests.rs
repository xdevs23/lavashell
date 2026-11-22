// SPDX-License-Identifier: GPL-3.0-only

use crate::EventLoopFailure;
use lavashell_error_chain::message_chain;
use std::io;

#[derive(Debug, thiserror::Error)]
#[error("the watched pipe broke")]
struct PipeBroke(#[source] io::Error);

#[test]
fn an_input_or_output_failure_shows_the_input_or_output_error_once() {
    let failure = calloop::Error::IoError(io::Error::other("the descriptor table is full"));

    assert_eq!(
        message_chain(EventLoopFailure::from(failure)),
        "an input or output operation failed: the descriptor table is full"
    );
}

#[test]
fn an_event_source_failure_shows_every_cause_below_it() {
    let cause = PipeBroke(io::Error::other("the writer hung up"));
    let failure = calloop::Error::OtherError(Box::new(cause));

    assert_eq!(
        message_chain(EventLoopFailure::from(failure)),
        "an event source failed: the watched pipe broke: the writer hung up"
    );
}

#[test]
fn a_failed_insertion_shows_the_error_of_the_event_loop_once() {
    let failure = calloop::InsertError {
        inserted: (),
        error: calloop::Error::IoError(io::Error::other("the descriptor table is full")),
    };

    assert_eq!(
        message_chain(EventLoopFailure::from(failure)),
        "an input or output operation failed: the descriptor table is full"
    );
}
