// SPDX-License-Identifier: GPL-3.0-only
use std::time::Duration;

/// Whether a run is at rest: no new frame was shown or is due, and nothing waits for a deadline. The test platform
/// stops recording at rest, and the steps of an example run the next step at rest.
#[must_use]
pub fn is_at_rest(new_frame: bool, deadline: Option<Duration>) -> bool {
    !new_frame && deadline.is_none()
}
