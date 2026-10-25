// SPDX-License-Identifier: GPL-3.0-only
use super::{Animatable, Direction, Keyframes, MotionSpec, Playback, Trajectory, VectorSpace};
use crate::numbers::duration_from_nanos;
use std::time::Duration;

#[derive(Clone, PartialEq)]
pub enum Program<T: Animatable> {
    Target {
        from: T::Vector,
        to: T::Vector,
        velocity: T::Vector,
        spec: MotionSpec,
        /// The smallest change of a vector component that is visible in the view that shows the motion, as
        /// returned by `Animatable::visible_step`. A spring motion ends once it is within that step of `to`.
        step: f64,
    },
    Frames {
        frames: Keyframes<T>,
        playback: Playback,
    },
}

/// A program with the elapsed time at which it comes to rest, computed once. `rest` is `None` for a program
/// that plays forever.
#[derive(Clone, PartialEq)]
pub(crate) struct Scheduled<T: Animatable> {
    program: Program<T>,
    rest: Option<Duration>,
}

impl<T: Animatable> Scheduled<T> {
    pub(crate) fn new(program: Program<T>) -> Self {
        Self {
            rest: program.rest(),
            program,
        }
    }

    pub(crate) fn program(&self) -> &Program<T> {
        &self.program
    }

    pub(crate) fn rest(&self) -> Option<Duration> {
        self.rest
    }

    /// The sample of the program at the elapsed time `elapsed`. A motion toward a target ends at `rest`.
    pub(crate) fn sample(&self, elapsed: Duration) -> Sample<T> {
        self.program.sample(elapsed, self.rest)
    }

    /// This schedule with its program mapped into the vector space of `U`. The mapped program comes to rest
    /// at the same elapsed time.
    fn mapped<U: Animatable>(
        &self,
        end: &U,
        position: impl Fn(&T::Vector) -> U::Vector,
        direction: impl Fn(&T::Vector, &T::Vector) -> U::Vector,
    ) -> Scheduled<U> {
        Scheduled {
            program: self.program.mapped(end, position, direction),
            rest: self.rest,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    pub start: Duration,
    pub paused: Option<Duration>,
    /// The elapsed time of the program at `start`.
    pub offset: Duration,
    pub rate: f64,
}

impl Timing {
    /// The elapsed time of the program at `now`. At a rate so large that the time overflows, returns the
    /// longest `Duration`.
    #[must_use]
    pub fn elapsed(&self, now: Duration) -> Duration {
        let running = self.paused.unwrap_or(now).saturating_sub(self.start);
        Duration::try_from_secs_f64(running.as_secs_f64() * self.rate)
            .unwrap_or(Duration::MAX)
            .saturating_add(self.offset)
    }

    pub(crate) fn reaching(&self, elapsed: Duration) -> Option<Duration> {
        if self.paused.is_some() {
            return None;
        }
        let ahead = elapsed.saturating_sub(self.offset).as_secs_f64() / self.rate;
        self.start
            .checked_add(Duration::try_from_secs_f64(ahead).ok()?)
    }
}

#[derive(Clone, PartialEq)]
pub struct Playing<T: Animatable> {
    end: T,
    scheduled: Scheduled<T>,
    timing: Timing,
    starts: Duration,
    /// The time since the frame epoch at which the motion comes to rest, or `None` for a motion that plays
    /// forever or is paused.
    rests: Option<Duration>,
}

impl<T: Animatable> Playing<T> {
    pub fn new(end: T, program: Program<T>, timing: Timing) -> Self {
        Self::scheduled(end, Scheduled::new(program), timing)
    }

    pub(crate) fn scheduled(end: T, scheduled: Scheduled<T>, timing: Timing) -> Self {
        let delay = match scheduled.program() {
            Program::Target {
                spec: MotionSpec::Tween(tween),
                ..
            } => tween.delay,
            Program::Target { .. } | Program::Frames { .. } => Duration::ZERO,
        };
        Self {
            starts: timing.reaching(delay).unwrap_or(timing.start),
            rests: scheduled.rest().and_then(|rest| timing.reaching(rest)),
            end,
            scheduled,
            timing,
        }
    }

    pub fn end(&self) -> &T {
        &self.end
    }

    pub fn program(&self) -> &Program<T> {
        self.scheduled.program()
    }

    pub fn at(&self, now: Duration) -> T {
        let sample = self.scheduled.sample(self.timing.elapsed(now));
        (sample.frame.as_ref())
            .unwrap_or(&self.end)
            .at(sample.trajectory.position)
    }

    pub(crate) fn next_change(&self, now: Duration) -> Option<Duration> {
        if self.timing.paused.is_some() || self.rests.is_some_and(|rests| rests <= now) {
            return None;
        }
        Some(self.starts.max(now))
    }

    /// This motion mapped into the vector space of `U`, ending at `end`. `position` must be an affine map from
    /// the vectors of `T` to the vectors of `U`. The mapped motion comes to rest at the same time as this motion.
    pub(crate) fn mapped<U: Animatable>(
        &self,
        end: U,
        position: impl Fn(&T::Vector) -> U::Vector,
    ) -> Playing<U> {
        // Velocities and tangents are differences, so only the linear part of the map applies to them.
        let direction =
            |base: &T::Vector, vector: &T::Vector| position(&base.add(vector)).sub(&position(base));
        let scheduled = self.scheduled.mapped(&end, &position, direction);
        Playing::scheduled(end, scheduled, self.timing)
    }

    /// This motion projected into the vector space of `U`, ending at `end`. `select` must be a linear map. The
    /// projected motion comes to rest at the same time as this motion.
    pub(crate) fn projected<U: Animatable>(
        &self,
        end: U,
        select: impl Fn(&T::Vector) -> U::Vector,
    ) -> Playing<U> {
        Playing {
            scheduled: (self.scheduled).mapped(&end, &select, |_, vector| select(vector)),
            end,
            timing: self.timing,
            starts: self.starts,
            rests: self.rests,
        }
    }
}

pub(crate) struct Sample<T: Animatable> {
    /// The value of the keyframe that ends the current segment, or `None` for a motion toward a target.
    pub frame: Option<T>,
    pub trajectory: Trajectory<T::Vector>,
    pub ended: Option<Duration>,
}

impl<T: Animatable> Program<T> {
    /// The sample of the program at the elapsed time `elapsed`. A motion toward a target ends at the elapsed
    /// time `rest`.
    fn sample(&self, elapsed: Duration, rest: Option<Duration>) -> Sample<T> {
        match self {
            Self::Target {
                from,
                to,
                velocity,
                spec,
                ..
            } => {
                let ended = rest.filter(|rest| elapsed >= *rest);
                let trajectory = if ended.is_some() {
                    Trajectory::at_rest(to.clone())
                } else {
                    let (position, velocity) = target(from, to, velocity, *spec, elapsed);
                    Trajectory {
                        position,
                        velocity,
                        end: to.clone(),
                    }
                };
                Sample {
                    frame: None,
                    trajectory,
                    ended,
                }
            }
            Self::Frames { frames, playback } => keyframes(frames, *playback, elapsed),
        }
    }

    fn rest(&self) -> Option<Duration> {
        match self {
            Self::Target {
                from,
                to,
                velocity,
                spec,
                step,
            } => match spec {
                MotionSpec::Instant => Some(Duration::ZERO),
                MotionSpec::Tween(tween) => Some(tween.duration.saturating_add(tween.delay)),
                MotionSpec::Spring(spring) => spring.rest(&from.sub(to), velocity, *step),
            },
            Self::Frames { frames, playback } => {
                (playback.repeat.cycles()).map(|cycles| frames.duration().saturating_mul(cycles))
            }
        }
    }

    fn mapped<U: Animatable>(
        &self,
        end: &U,
        position: impl Fn(&T::Vector) -> U::Vector,
        direction: impl Fn(&T::Vector, &T::Vector) -> U::Vector,
    ) -> Program<U> {
        match self {
            Self::Target {
                from,
                to,
                velocity,
                spec,
                step,
            } => Program::Target {
                from: position(from),
                to: position(to),
                velocity: direction(to, velocity),
                spec: *spec,
                step: *step,
            },
            Self::Frames { frames, playback } => Program::Frames {
                frames: frames.mapped(
                    |value| end.at(position(&value.vector())),
                    |value, tangent| direction(&value.vector(), tangent),
                ),
                playback: *playback,
            },
        }
    }
}

/// The position and the velocity of a motion from `from` toward `to` at the elapsed time `elapsed`.
fn target<V: VectorSpace>(
    from: &V,
    to: &V,
    velocity: &V,
    spec: MotionSpec,
    elapsed: Duration,
) -> (V, V) {
    match spec {
        MotionSpec::Instant => (to.clone(), V::zero()),
        MotionSpec::Spring(spring) => {
            let (offset, speed) = spring.sample(&from.sub(to), velocity, elapsed.as_secs_f64());
            (to.add(&offset), speed)
        }
        MotionSpec::Tween(tween) => {
            let time = elapsed.saturating_sub(tween.delay);
            if elapsed < tween.delay {
                return (from.clone(), V::zero());
            }
            if time >= tween.duration {
                return (to.clone(), V::zero());
            }
            let seconds = tween.duration.as_secs_f64();
            let u = time.as_secs_f64() / seconds;
            let (progress, slope) = tween.easing.sample(u);
            let delta = to.sub(from);
            // The quintic corrections at the ends of the curve start at the velocity that the value had when
            // the target changed. They end at rest with zero acceleration.
            let (v0, v1) = if tween.preserve_velocity {
                (
                    velocity
                        .scale(seconds)
                        .sub(&delta.scale(tween.easing.sample(0.0).1)),
                    delta.scale(-tween.easing.sample(1.0).1),
                )
            } else {
                (V::zero(), V::zero())
            };
            let h0 = u - 6.0 * u.powi(3) + 8.0 * u.powi(4) - 3.0 * u.powi(5);
            let h1 = -4.0 * u.powi(3) + 7.0 * u.powi(4) - 3.0 * u.powi(5);
            let d0 = 1.0 - 18.0 * u * u + 32.0 * u.powi(3) - 15.0 * u.powi(4);
            let d1 = -12.0 * u * u + 28.0 * u.powi(3) - 15.0 * u.powi(4);
            let p = from
                .add(&delta.scale(progress))
                .add(&v0.scale(h0))
                .add(&v1.scale(h1));
            let v = delta
                .scale(slope)
                .add(&v0.scale(d0))
                .add(&v1.scale(d1))
                .scale(1.0 / seconds);
            (p, v)
        }
    }
}

fn keyframes<T: Animatable>(
    frames: &Keyframes<T>,
    playback: Playback,
    elapsed: Duration,
) -> Sample<T> {
    let duration = frames.duration();
    let cycles = elapsed.as_nanos() / duration.as_nanos();
    let count = playback.repeat.cycles();
    let ended = count
        .filter(|count| cycles >= u128::from(*count))
        .map(|count| duration.saturating_mul(count));
    let cycle = match (ended, count) {
        (Some(_), Some(count)) => u128::from(count) - 1,
        _ => cycles,
    };
    let reverse = playback.direction == Direction::Alternate && cycle % 2 == 1;
    let within = if ended.is_some() {
        duration
    } else {
        duration_from_nanos(elapsed.as_nanos() % duration.as_nanos())
    };
    let at = if reverse {
        duration.saturating_sub(within)
    } else {
        within
    };
    let (frame, sampled) = frames.sample(at);
    let velocity = match ended {
        Some(_) => T::Vector::zero(),
        None if reverse => sampled.velocity.scale(-1.0),
        None => sampled.velocity,
    };
    Sample {
        frame: Some(frame),
        trajectory: Trajectory {
            velocity,
            ..sampled
        },
        ended,
    }
}
