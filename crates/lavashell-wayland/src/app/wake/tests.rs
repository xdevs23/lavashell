// SPDX-License-Identifier: GPL-3.0-only
use super::next_wake;
use std::time::Duration;

#[test]
fn a_deadline_of_the_view_wakes_the_loop_at_the_deadline() {
    assert_eq!(
        next_wake(
            Duration::from_millis(100),
            Duration::from_millis(116),
            Some(Duration::from_millis(150)),
            None
        ),
        Some(Duration::from_millis(150))
    );
}

#[test]
fn a_change_of_the_scene_wakes_the_loop_one_presentation_lead_early() {
    assert_eq!(
        next_wake(
            Duration::from_millis(100),
            Duration::from_millis(116),
            None,
            Some(Duration::from_millis(150))
        ),
        Some(Duration::from_millis(134))
    );
}

#[test]
fn a_deadline_within_the_presentation_lead_still_wakes_the_loop() {
    assert_eq!(
        next_wake(
            Duration::from_millis(100),
            Duration::from_millis(116),
            Some(Duration::from_millis(110)),
            None
        ),
        Some(Duration::from_millis(110))
    );
}

#[test]
fn the_earlier_of_both_wakes_the_loop() {
    assert_eq!(
        next_wake(
            Duration::from_millis(100),
            Duration::from_millis(116),
            Some(Duration::from_millis(150)),
            Some(Duration::from_millis(140))
        ),
        Some(Duration::from_millis(124))
    );
}

#[test]
fn a_time_that_has_passed_wakes_nothing() {
    assert_eq!(
        next_wake(
            Duration::from_millis(100),
            Duration::from_millis(116),
            Some(Duration::from_millis(100)),
            Some(Duration::from_millis(110))
        ),
        None
    );
}
