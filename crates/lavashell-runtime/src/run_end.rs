// SPDX-License-Identifier: GPL-3.0-only
use std::time::Duration;

/// When a run ends: once nothing is open, once the time set by `exit_after` has passed since the start, or once the
/// predicate set by `exit_when` returns true.
#[derive(Default)]
pub struct RunEnd {
    exit_after: Option<Duration>,
    exit_when: Option<Box<dyn FnMut() -> bool>>,
}

impl RunEnd {
    pub fn exit_after(&mut self, duration: Duration) {
        self.exit_after = Some(duration);
    }

    pub fn exit_when(&mut self, predicate: impl FnMut() -> bool + 'static) {
        self.exit_when = Some(Box::new(predicate));
    }

    /// Whether the run ends at `elapsed` after its start. The predicate of `exit_when` runs only when nothing else
    /// ends the run.
    pub fn finished(&mut self, elapsed: Duration, anything_open: bool) -> bool {
        !anything_open
            || self.exit_after.is_some_and(|limit| elapsed >= limit)
            || self.exit_when.as_mut().is_some_and(|predicate| predicate())
    }

    /// The time left at `elapsed` until the time set by `exit_after`, or `None` without that time.
    #[must_use]
    pub fn exit_in(&self, elapsed: Duration) -> Option<Duration> {
        (self.exit_after).map(|limit| limit.saturating_sub(elapsed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn a_run_ends_once_nothing_is_open() {
        let mut end = RunEnd::default();
        assert!(!end.finished(SECOND, true));
        assert!(end.finished(SECOND, false));
        assert_eq!(end.exit_in(SECOND), None);
    }

    #[test]
    fn a_run_ends_at_the_time_set_by_exit_after() {
        let mut end = RunEnd::default();
        end.exit_after(2 * SECOND);
        assert_eq!(end.exit_in(SECOND), Some(SECOND));
        assert!(!end.finished(SECOND, true));
        assert!(end.finished(2 * SECOND, true));
        assert_eq!(end.exit_in(3 * SECOND), Some(Duration::ZERO));
    }

    #[test]
    fn the_exit_predicate_runs_only_while_nothing_else_ends_the_run() {
        let calls = Rc::new(Cell::new(0));
        let counted = calls.clone();
        let mut end = RunEnd::default();
        end.exit_when(move || {
            counted.set(counted.get() + 1);
            counted.get() == 2
        });
        assert!(end.finished(SECOND, false));
        assert_eq!(calls.get(), 0);
        assert!(!end.finished(SECOND, true));
        assert!(end.finished(SECOND, true));
        assert_eq!(calls.get(), 2);
    }
}
