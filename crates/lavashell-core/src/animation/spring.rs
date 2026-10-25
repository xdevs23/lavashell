// SPDX-License-Identifier: GPL-3.0-only
use super::VectorSpace;
use super::motion_scale::MotionScale;
use std::time::Duration;

/// One nanosecond, the smallest step of `Duration`.
const TICK: Duration = Duration::from_nanos(1);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    frequency: f64,
    damping: f64,
}

/// The reason why the parameters given to `Spring::new` or `Spring::physical` do not form a spring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SpringError {
    #[error("the response of a spring must be longer than zero")]
    ZeroResponse,
    #[error("the damping ratio of a spring must be finite and positive")]
    Damping,
    #[error("the mass, stiffness and damping of a spring must be finite and positive")]
    Physical,
    #[error("the frequency and the damping of a spring are out of range")]
    OutOfRange,
}

impl Spring {
    /// A spring whose undamped oscillation has the period `response`. A `damping_ratio` of 1 is critical
    /// damping, at which the motion comes to rest without overshoot in the shortest time. The `spring!` macro
    /// checks constant arguments when the program is compiled.
    ///
    /// # Errors
    ///
    /// Fails when the response is zero, when the damping ratio is not finite and positive, or when the product
    /// of the frequency and the damping ratio is not finite.
    pub const fn new(response: Duration, damping_ratio: f64) -> Result<Self, SpringError> {
        if response.is_zero() {
            return Err(SpringError::ZeroResponse);
        }
        if !damping_ratio.is_finite() || damping_ratio <= 0.0 {
            return Err(SpringError::Damping);
        }
        let frequency = std::f64::consts::TAU / response.as_secs_f64();
        if !(frequency * damping_ratio).is_finite() {
            return Err(SpringError::OutOfRange);
        }
        Ok(Self {
            frequency,
            damping: damping_ratio,
        })
    }

    /// A spring from a mass, a stiffness and a damping coefficient.
    ///
    /// # Errors
    ///
    /// Fails when a parameter is not finite and positive, or when the frequency or the damping ratio computed
    /// from the parameters is out of range.
    pub fn physical(mass: f64, stiffness: f64, damping: f64) -> Result<Self, SpringError> {
        if ![mass, stiffness, damping]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
        {
            return Err(SpringError::Physical);
        }
        let frequency = stiffness.sqrt() / mass.sqrt();
        let ratio = (damping / mass.sqrt()) / (2.0 * stiffness.sqrt());
        if !frequency.is_finite()
            || frequency <= 0.0
            || !ratio.is_finite()
            || ratio <= 0.0
            || !(frequency * ratio).is_finite()
        {
            return Err(SpringError::OutOfRange);
        }
        Ok(Self {
            frequency,
            damping: ratio,
        })
    }

    /// The position relative to the end and the velocity after `seconds` of a motion that starts at
    /// `displacement` from its end with the velocity `velocity`. A component beyond the range of f64
    /// saturates at `±f64::MAX`.
    #[must_use]
    pub fn sample<V: VectorSpace>(&self, displacement: &V, velocity: &V, seconds: f64) -> (V, V) {
        MotionScale::sampled([displacement, velocity], |[displacement, velocity]| {
            self.motion(displacement, velocity, seconds)
        })
    }

    fn motion<V: VectorSpace>(&self, displacement: &V, velocity: &V, seconds: f64) -> (V, V) {
        let t = seconds.max(0.0);
        if self.damping < 1.0 {
            self.underdamped(displacement, velocity, t)
        } else {
            self.overdamped(displacement, velocity, t)
        }
    }

    /// Whether a motion at `position` from its end with the velocity `speed` has ended: its extent is at most
    /// `step`, the smallest visible change. The extent combines position and velocity, so a motion that passes its
    /// end at speed does not end there. The extent never grows, because the damping only removes energy.
    fn settled<V: VectorSpace>(&self, position: &V, speed: &V, step: f64) -> bool {
        self.extent(position, speed) <= step
    }

    /// How far a motion can still move: the hypotenuse of the largest component of `position` and the largest
    /// component of `speed` divided by the frequency, which is the distance moved per radian of phase.
    fn extent<V: VectorSpace>(&self, position: &V, speed: &V) -> f64 {
        position.max_abs().hypot(speed.max_abs() / self.frequency)
    }

    fn underdamped<V: VectorSpace>(&self, displacement: &V, velocity: &V, t: f64) -> (V, V) {
        let damped_frequency = self.frequency * (1.0 - self.damping * self.damping).sqrt();
        let decay_rate = self.damping * self.frequency;
        let sine_weight = velocity
            .add(&displacement.scale(decay_rate))
            .scale(1.0 / damped_frequency);
        let (sin, cos) = (damped_frequency * t).sin_cos();
        let decay = (-decay_rate * t).exp();
        let oscillation = displacement.scale(cos).add(&sine_weight.scale(sin));
        let speed = sine_weight
            .scale(damped_frequency * cos)
            .sub(&displacement.scale(damped_frequency * sin))
            .sub(&oscillation.scale(decay_rate));
        (oscillation.scale(decay), speed.scale(decay))
    }

    /// Critical and overdamped motion, which has two real decay rates. The position decays at the slow rate, and
    /// the velocity has an extra term that decays at the fast rate. The coefficients are finite also at critical
    /// damping, where the two rates are equal.
    fn overdamped<V: VectorSpace>(&self, displacement: &V, velocity: &V, t: f64) -> (V, V) {
        let root = (self.damping - 1.0).sqrt() * (self.damping + 1.0).sqrt();
        let slow_rate = self.frequency / (self.damping + root);
        let fast_rate = self.frequency * (self.damping + root);
        // The part of the initial velocity that the slow decay of the displacement does not account for.
        let drive = velocity.add(&displacement.scale(slow_rate));
        let spread = (fast_rate - slow_rate) * t;
        let position = displacement
            .add(&drive.scale(t * saturation(spread)))
            .scale((-slow_rate * t).exp());
        let speed = position
            .scale(-slow_rate)
            .add(&drive.scale((-fast_rate * t).exp()));
        (position, speed)
    }

    /// The first elapsed time, to the nanosecond, at which the motion from `from` toward `to` with the velocity
    /// `velocity` has ended within `step`. Returns `None` when that time is past the range of `Duration`.
    pub(crate) fn rest<V: VectorSpace>(
        &self,
        from: &V,
        to: &V,
        velocity: &V,
        step: f64,
    ) -> Option<Duration> {
        let scale = MotionScale::of(&[from, to, velocity]);
        let displacement = scale.reduced(from).sub(&scale.reduced(to));
        let velocity = scale.reduced(velocity);
        self.first_time(|time| {
            let (position, speed) = self.motion(&displacement, &velocity, time.as_secs_f64());
            self.settled(&scale.restored(&position), &scale.restored(&speed), step)
        })
    }

    /// The first elapsed time, to the nanosecond, at which `done` returns `true`, or `None` when that time is past
    /// the range of `Duration`. Once `done` returns `true` for a time, it must return `true` for every later time,
    /// as it does for a motion of this spring that has come close enough to its end. The search doubles the time
    /// from one period of the spring until `done` returns `true`, then bisects the last interval.
    pub fn first_time(&self, done: impl Fn(Duration) -> bool) -> Option<Duration> {
        if done(Duration::ZERO) {
            return Some(Duration::ZERO);
        }
        let period = Duration::try_from_secs_f64(std::f64::consts::TAU / self.frequency).ok()?;
        let (mut moving, mut resting) = (Duration::ZERO, period.max(TICK));
        while !done(resting) {
            moving = resting;
            resting = resting.checked_mul(2)?;
        }
        loop {
            let gap = resting.saturating_sub(moving);
            if gap <= TICK {
                return Some(resting);
            }
            let midpoint = moving + gap / 2;
            if done(midpoint) {
                resting = midpoint;
            } else {
                moving = midpoint;
            }
        }
    }
}

/// `(1 - e^-x) / x`, with the limit 1 at zero. `exp_m1` computes the numerator exactly for small `x`, where
/// `1 - e^-x` computed directly would lose its digits to cancellation.
fn saturation(x: f64) -> f64 {
    if x == 0.0 { 1.0 } else { -(-x).exp_m1() / x }
}

/// A `Spring` from a constant response and damping ratio, checked when the program is compiled.
#[macro_export]
macro_rules! spring {
    ($response:expr, $damping_ratio:expr $(,)?) => {
        $crate::constant!($crate::animation::Spring::new($response, $damping_ratio))
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::Vector;
    use crate::float_asserts::assert_f64_eq;
    use crate::test_error::{Required, TestResult};

    #[test]
    fn unit_mass_with_stiffness_100_and_damping_20_is_critically_damped() -> TestResult {
        let spring = Spring::physical(1.0, 100.0, 20.0)?;
        assert_f64_eq(spring.frequency, 100.0_f64.sqrt());
        assert_f64_eq(spring.damping, 1.0);
        Ok(())
    }

    #[test]
    fn a_spring_needs_a_response_a_damping_and_a_frequency_in_range() {
        let response = Duration::from_millis(400);
        assert_eq!(
            Spring::new(Duration::ZERO, 1.0),
            Err(SpringError::ZeroResponse)
        );
        for damping in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(Spring::new(response, damping), Err(SpringError::Damping));
        }
        assert_eq!(
            Spring::new(Duration::from_nanos(1), f64::MAX),
            Err(SpringError::OutOfRange)
        );
        assert_eq!(Spring::physical(1.0, 0.0, 1.0), Err(SpringError::Physical));
        assert_eq!(
            Spring::physical(f64::from_bits(1), f64::MAX, 1.0),
            Err(SpringError::OutOfRange)
        );
    }

    #[test]
    fn a_motion_rests_from_the_first_nanosecond_at_which_it_lies_within_the_step() -> TestResult {
        let spring = Spring::new(Duration::from_millis(400), 0.5)?;
        let still = Vector([0.0]);
        let settled = |displacement: &Vector<1>, time: Duration| {
            let (position, speed) = spring.sample(displacement, &still, time.as_secs_f64());
            spring.settled(&position, &speed, 1.0)
        };
        let near = Vector([100.0]);
        let rest = spring
            .rest(&near, &still, &still, 1.0)
            .required("rest time")?;
        assert!(settled(&near, rest));
        let before = rest.checked_sub(TICK).required("time before the rest")?;
        assert!(!settled(&near, before));
        let far = Vector([10_000.0]);
        let far_rest = spring
            .rest(&far, &still, &still, 1.0)
            .required("rest time")?;
        assert!(far_rest > rest);
        Ok(())
    }

    #[test]
    fn the_first_time_is_found_to_the_nanosecond() -> TestResult {
        let spring = Spring::new(Duration::from_millis(400), 1.0)?;
        let time = Duration::from_nanos(1_234_567_891);
        assert_eq!(spring.first_time(|now| now >= time), Some(time));
        assert_eq!(spring.first_time(|_| true), Some(Duration::ZERO));
        assert_eq!(spring.first_time(|_| false), None);
        Ok(())
    }

    #[test]
    fn a_motion_without_displacement_and_velocity_is_done_at_once() -> TestResult {
        let spring = Spring::new(Duration::from_millis(400), 1.0)?;
        let still = Vector([0.0]);
        let (position, speed) = spring.sample(&still, &still, 0.0);
        assert_eq!((position, speed), (still, still));
        assert!(spring.settled(&position, &speed, 0.0));
        assert_eq!(
            spring.rest(&still, &still, &still, 0.0),
            Some(Duration::ZERO)
        );
        Ok(())
    }

    #[test]
    fn a_motion_from_the_largest_distance_moves_before_it_rests() -> TestResult {
        let spring = Spring::new(Duration::from_millis(400), 0.5)?;
        let (far, still) = (Vector([f64::MAX]), Vector([0.0]));
        let rest = spring
            .rest(&far, &still, &still, 1.0)
            .required("rest time")?;
        assert!(rest > Duration::ZERO);
        let (position, speed) = spring.sample(&far, &still, 0.1);
        assert!(position.0[0].abs() < f64::MAX, "{position:?}");
        assert!(speed.0[0] < 0.0 && speed.0[0].is_finite(), "{speed:?}");
        Ok(())
    }

    #[test]
    fn a_motion_started_by_velocity_alone_settles() -> TestResult {
        let spring = Spring::new(Duration::from_millis(400), 1.0)?;
        let (still, pushed) = (Vector([0.0]), Vector([300.0]));
        let rest = spring
            .rest(&still, &still, &pushed, 1.0)
            .required("rest time")?;
        assert!(rest > Duration::ZERO);
        let (position, speed) = spring.sample(&still, &pushed, rest.as_secs_f64());
        assert!(spring.settled(&position, &speed, 1.0));
        Ok(())
    }

    #[test]
    fn overdamped_motion_approaches_critical_motion_as_the_damping_approaches_critical()
    -> TestResult {
        let motion = |damping: f64| -> Result<_, SpringError> {
            let spring = Spring::new(Duration::from_millis(400), damping)?;
            Ok(spring.sample(&Vector([100.0]), &Vector([-40.0]), 0.3))
        };
        let (critical, critical_speed) = motion(1.0)?;
        let gaps =
            [1e-6, 1e-9, 1e-12]
                .into_iter()
                .map(|excess| {
                    let (position, speed) = motion(1.0 + excess)?;
                    Ok((position.0[0] - critical.0[0]).abs()
                        + (speed.0[0] - critical_speed.0[0]).abs())
                })
                .collect::<Result<Vec<f64>, SpringError>>()?;
        assert!(
            gaps.is_sorted_by(|wider, narrower| narrower < wider),
            "{gaps:?}"
        );
        Ok(())
    }

    #[test]
    fn a_heavily_overdamped_motion_follows_its_slow_decay() -> TestResult {
        let spring = Spring::new(Duration::from_millis(400), 1000.0)?;
        let root = (spring.damping - 1.0).sqrt() * (spring.damping + 1.0).sqrt();
        let slow_rate = spring.frequency / (spring.damping + root);
        let (position, speed) = spring.sample(&Vector([1.0]), &Vector([0.0]), 2.0);
        assert_f64_eq(speed.0[0] / position.0[0], -slow_rate);
        Ok(())
    }
}
