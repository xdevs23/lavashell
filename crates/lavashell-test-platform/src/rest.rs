// SPDX-License-Identifier: GPL-3.0-only
use std::time::Duration;

#[must_use]
pub fn is_at_rest(new_frame: bool, deadline: Option<Duration>) -> bool {
    !new_frame && deadline.is_none()
}
