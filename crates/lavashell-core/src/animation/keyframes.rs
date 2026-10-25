// SPDX-License-Identifier: GPL-3.0-only
use super::{Animatable, Easing, Trajectory, VectorSpace};
use std::time::Duration;

#[derive(Clone, PartialEq)]
pub struct Keyframe<T: Animatable> {
    pub at: Duration,
    pub value: T,
    pub easing: Easing,
    /// The velocity at this keyframe, in units per second. A segment with a tangent at either end follows a
    /// cubic Hermite curve. A segment without tangents follows the easing of its first keyframe.
    pub tangent: Option<T::Vector>,
}

impl<T: Animatable> Keyframe<T> {
    pub fn new(at: Duration, value: T) -> Self {
        Self {
            at,
            value,
            easing: Easing::Smooth,
            tangent: None,
        }
    }

    #[must_use]
    pub fn easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    #[must_use]
    pub fn tangent(mut self, tangent: T::Vector) -> Self {
        self.tangent = Some(tangent);
        self
    }
}

/// The error returned by `Keyframes::new` for keyframes that do not form an animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum KeyframesError {
    #[error("keyframes need at least two strictly increasing times, beginning at zero")]
    Times,
    #[error("keyframe values and tangents must be finite")]
    NotFinite,
}

#[derive(Clone, PartialEq)]
pub struct Keyframes<T: Animatable> {
    first: Keyframe<T>,
    middle: Vec<Keyframe<T>>,
    last: Keyframe<T>,
}

impl<T: Animatable> Keyframes<T> {
    /// # Errors
    ///
    /// Fails when there are fewer than two frames, when their times do not begin at zero or do not
    /// strictly increase, or when a value or a tangent is not finite.
    pub fn new(frames: impl IntoIterator<Item = Keyframe<T>>) -> Result<Self, KeyframesError> {
        let mut frames = frames.into_iter();
        let first = frames.next().ok_or(KeyframesError::Times)?;
        let mut middle: Vec<_> = frames.collect();
        let last = middle.pop().ok_or(KeyframesError::Times)?;
        let keyframes = Self {
            first,
            middle,
            last,
        };
        if !keyframes.first.at.is_zero() || keyframes.segments().any(|(a, b)| a.at >= b.at) {
            return Err(KeyframesError::Times);
        }
        if (keyframes.frames())
            .any(|f| !f.value.vector().finite() || f.tangent.as_ref().is_some_and(|v| !v.finite()))
        {
            return Err(KeyframesError::NotFinite);
        }
        Ok(keyframes)
    }

    fn frames(&self) -> impl Iterator<Item = &Keyframe<T>> {
        std::iter::once(&self.first)
            .chain(&self.middle)
            .chain(std::iter::once(&self.last))
    }

    /// Each keyframe paired with the one after it.
    fn segments(&self) -> impl Iterator<Item = (&Keyframe<T>, &Keyframe<T>)> {
        self.frames().zip(self.frames().skip(1))
    }

    pub fn duration(&self) -> Duration {
        self.last.at
    }

    pub fn first(&self) -> &T {
        &self.first.value
    }

    pub fn last(&self) -> &T {
        &self.last.value
    }

    pub(crate) fn mapped<U: Animatable>(
        &self,
        value: impl Fn(&T) -> U,
        tangent: impl Fn(&T, &T::Vector) -> U::Vector,
    ) -> Keyframes<U> {
        let map = |frame: &Keyframe<T>| Keyframe {
            at: frame.at,
            value: value(&frame.value),
            easing: frame.easing,
            tangent: (frame.tangent.as_ref()).map(|vector| tangent(&frame.value, vector)),
        };
        Keyframes {
            first: map(&self.first),
            middle: self.middle.iter().map(map).collect(),
            last: map(&self.last),
        }
    }

    /// Returns the value of the keyframe that ends the segment at `elapsed`, with every part that
    /// only the keyframe at the start of the segment has added at zero strength. Also returns the
    /// trajectory at `elapsed` in the vector space of that value. After the last keyframe, returns
    /// the last value at rest.
    pub fn sample(&self, elapsed: Duration) -> (T, Trajectory<T::Vector>) {
        let Some((before, after)) = self.segments().find(|(_, after)| elapsed < after.at) else {
            return (
                self.last().clone(),
                Trajectory::at_rest(self.last().vector()),
            );
        };
        let seconds = after.at.saturating_sub(before.at).as_secs_f64();
        let along = elapsed.saturating_sub(before.at).as_secs_f64() / seconds;
        let start = Trajectory {
            velocity: before.tangent.clone().unwrap_or_else(T::Vector::zero),
            ..Trajectory::at_rest(before.value.vector())
        };
        let frame = after.value.spanning(&before.value);
        let Trajectory {
            position: from,
            velocity: tangent,
            end,
        } = frame.retarget(&before.value, start);
        let delta = end.sub(&from);
        let (position, velocity) = if before.tangent.is_some() || after.tangent.is_some() {
            let v0 = tangent.scale(seconds);
            let v1 = after
                .tangent
                .clone()
                .unwrap_or_else(T::Vector::zero)
                .scale(seconds);
            let smooth = along * along * (3.0 - 2.0 * along);
            let p = from
                .add(&delta.scale(smooth))
                .add(&v0.scale(along * (1.0 - along).powi(2)))
                .add(&v1.scale(along * along * (along - 1.0)));
            let v = delta
                .scale(6.0 * along * (1.0 - along))
                .add(&v0.scale(1.0 - 4.0 * along + 3.0 * along * along))
                .add(&v1.scale(3.0 * along * along - 2.0 * along))
                .scale(1.0 / seconds);
            (p, v)
        } else {
            let (progress, slope) = before.easing.sample(along);
            (
                from.add(&delta.scale(progress)),
                delta.scale(slope / seconds),
            )
        };
        let trajectory = Trajectory {
            position,
            velocity,
            end,
        };
        (frame, trajectory)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Repeat {
    #[default]
    Once,
    Count(u32),
    Forever,
}

impl Repeat {
    /// The number of cycles to play, or `None` for an animation that repeats forever. `Count(0)`
    /// plays one cycle.
    #[must_use]
    pub fn cycles(self) -> Option<u32> {
        match self {
            Self::Once => Some(1),
            Self::Count(count) => Some(count.max(1)),
            Self::Forever => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Forward,
    Alternate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Playback {
    pub repeat: Repeat,
    pub direction: Direction,
}
