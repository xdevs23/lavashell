// SPDX-License-Identifier: GPL-3.0-only
use super::{InputEvent, InputTime, TouchMove, TouchSample};
use std::time::Duration;

#[must_use]
pub fn input_time_at_millis(millis: u64) -> InputTime {
    InputTime(Duration::from_millis(millis))
}

#[must_use]
pub fn touch_move_at(sample: TouchSample, millis: u64) -> TouchMove {
    TouchMove {
        sample,
        time: input_time_at_millis(millis),
    }
}

#[must_use]
pub fn touch_move(sample: TouchSample) -> InputEvent {
    InputEvent::TouchMove(touch_move_at(sample, 0))
}
