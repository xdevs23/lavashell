// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestResult;
use std::time::Duration;

#[test]
fn springs_follow_a_runge_kutta_reference_on_both_sides_of_critical_damping() -> TestResult {
    const RATE: u32 = 48_000;
    let step = 1.0 / f64::from(RATE);
    let response = 0.45;
    let frequency = std::f64::consts::TAU / response;
    let (start, start_velocity): (f64, f64) = (720.0, 93.0);

    for damping in [0.15, 0.85, 0.999_999_9, 1.0, 1.000_000_1, 2.0, 8.0] {
        let spring = Spring::new(Duration::from_secs_f64(response), damping)?;
        let acceleration =
            |x: f64, v: f64| -frequency * frequency * x - 2.0 * damping * frequency * v;
        let (mut x, mut v) = (start, start_velocity);
        let mut peak = start.abs().max(start_velocity.abs() / frequency);
        let mut reference = Vec::new();

        for _ in 0..RATE {
            let (k1x, k1v) = (v, acceleration(x, v));
            let (k2x, k2v) = (
                v + k1v * step / 2.0,
                acceleration(x + k1x * step / 2.0, v + k1v * step / 2.0),
            );
            let (k3x, k3v) = (
                v + k2v * step / 2.0,
                acceleration(x + k2x * step / 2.0, v + k2v * step / 2.0),
            );
            let (k4x, k4v) = (v + k3v * step, acceleration(x + k3x * step, v + k3v * step));
            x += step / 6.0 * (k1x + 2.0 * k2x + 2.0 * k3x + k4x);
            v += step / 6.0 * (k1v + 2.0 * k2v + 2.0 * k3v + k4v);
            peak = peak.max(x.abs()).max(v.abs() / frequency);
            reference.push((x, v));
        }

        // Every step of the reference rounds the position and the velocity to the spacing of f64 at the largest
        // value they take. The whole run is therefore off by at most RATE of those spacings, far above the
        // truncation error of the method at this step. The velocity is checked with that bound times the
        // frequency, because a velocity is as large as a position times the frequency.
        let rounding = f64::from(RATE) * f64::EPSILON * peak;
        for (i, (expected_position, expected_speed)) in (1..=RATE).zip(reference) {
            let (value, speed) = spring.sample(
                &Vector([start]),
                &Vector([start_velocity]),
                f64::from(i) * step,
            );
            assert!(
                (value.0[0] - expected_position).abs() <= rounding,
                "position at damping {damping}"
            );
            assert!(
                (speed.0[0] - expected_speed).abs() <= rounding * frequency,
                "velocity at damping {damping}"
            );
        }
    }
    Ok(())
}
