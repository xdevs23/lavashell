// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn the_system_clock_waits_for_the_time_left_until_a_deadline() {
    let clock = SystemClock::start();
    let waiter = clock.waiter(Arc::new(|| {}));
    assert_eq!(waiter.wait_until(Some(Duration::ZERO)), Timeout::Passed);
    assert_eq!(waiter.wait_until(None), Timeout::Unbounded);
    let hour = Duration::from_secs(3600);
    let far = clock.now() + hour;
    assert!(matches!(waiter.wait_until(Some(far)), Timeout::After(left) if left <= hour));
}
