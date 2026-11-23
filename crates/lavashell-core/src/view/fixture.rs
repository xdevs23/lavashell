// SPDX-License-Identifier: GPL-3.0-only
use super::{InputEvent, InputTime, TouchMove, TouchSample};
use std::time::Duration;

#[must_use]
pub fn touch_move(sample: TouchSample) -> InputEvent {
    InputEvent::TouchMove(TouchMove {
        sample,
        time: InputTime(Duration::ZERO),
    })
}
