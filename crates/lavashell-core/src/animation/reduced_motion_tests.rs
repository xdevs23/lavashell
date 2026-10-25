// SPDX-License-Identifier: GPL-3.0-only
use super::tests::ms;
use super::*;
use crate::float_asserts::assert_f64_eq;
use crate::geometry::Point;
use crate::test_error::{Required, TestResult};
use crate::units::DpExt;

#[test]
fn reduced_motion_cancels_work_and_reaches_exact_targets() -> TestResult {
    let motion = Motion::default();
    let a = motion.animation(Point::new(0.dp(), 0.dp()))?;
    a.animate_to(Point::new(300.dp(), 20.dp()), Spring::new(ms(450), 0.85)?)?;
    motion.reduced_motion(true);
    motion.advance(ms(1));
    assert_eq!(a.get(), Point::new(300.dp(), 20.dp()));
    assert!(!motion.is_active());
    Ok(())
}

#[test]
fn reduced_motion_records_when_the_motion_it_skips_would_have_come_to_rest() -> TestResult {
    let spring = Spring::new(ms(450), 0.85)?;
    let moving = Motion::default();
    let a = moving.animation(0.0_f64)?;
    moving.advance(ms(100));
    a.animate_to(300.0, spring)?;
    let rest = moving.next_rest().required("rest of the motion")?;
    let reduced = Motion::default();
    let b = reduced.animation(0.0_f64)?;
    reduced.reduced_motion(true);
    reduced.advance(ms(100));
    assert_eq!(reduced.skipped_rest(), None);
    b.animate_to(300.0, spring)?;
    assert_f64_eq(b.get(), 300.0);
    assert_eq!(reduced.skipped_rest(), Some(rest));
    Ok(())
}
