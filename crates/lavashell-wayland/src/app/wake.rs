// SPDX-License-Identifier: GPL-3.0-only
use std::time::Duration;

/// The next time after `now` at which the event loop wakes: at `deadline`, or early enough before `change` for a
/// frame to show that change of the scene. A frame started at `now` shows the scene at `shown`, which is
/// `shown - now` later.
pub(super) fn next_wake(
    now: Duration,
    shown: Duration,
    deadline: Option<Duration>,
    change: Option<Duration>,
) -> Option<Duration> {
    let lead = shown.saturating_sub(now);
    [deadline, change.map(|change| change.saturating_sub(lead))]
        .into_iter()
        .flatten()
        .filter(|wake| *wake > now)
        .min()
}

#[cfg(test)]
mod tests;
