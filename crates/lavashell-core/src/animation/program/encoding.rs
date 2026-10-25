// SPDX-License-Identifier: GPL-3.0-only
//! Motions are read through the checks of their constructors. A motion is written without the times computed
//! from it, which are computed again when it is read.

use super::{Animatable, Keyframes, MotionError, MotionSpec, Playback, Playing, Program, Timing};
use crate::animation::PlaybackRateError;
use crate::animation::track::checked_rate;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::time::Duration;

#[derive(Deserialize)]
#[serde(
    rename = "Program",
    bound(deserialize = "T: Deserialize<'de>, T::Vector: Deserialize<'de>")
)]
pub(super) enum ProgramParts<T: Animatable> {
    Target {
        from: T::Vector,
        to: T::Vector,
        velocity: T::Vector,
        spec: MotionSpec,
        step: f64,
    },
    Frames {
        frames: Keyframes<T>,
        playback: Playback,
    },
}

impl<T: Animatable> TryFrom<ProgramParts<T>> for Program<T> {
    type Error = MotionError;

    fn try_from(parts: ProgramParts<T>) -> Result<Self, MotionError> {
        match parts {
            ProgramParts::Target {
                from,
                to,
                velocity,
                spec,
                step,
            } => Self::target(from, to, velocity, spec, step),
            ProgramParts::Frames { frames, playback } => Ok(Self::Frames { frames, playback }),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename = "Timing")]
pub(super) struct TimingParts {
    start: Duration,
    paused: Option<Duration>,
    offset: Duration,
    rate: f64,
}

impl TryFrom<TimingParts> for Timing {
    type Error = PlaybackRateError;

    fn try_from(parts: TimingParts) -> Result<Self, PlaybackRateError> {
        Ok(Self {
            start: parts.start,
            paused: parts.paused,
            offset: parts.offset,
            rate: checked_rate(parts.rate)?,
        })
    }
}

#[derive(Deserialize)]
#[serde(
    rename = "Playing",
    bound(deserialize = "T: Deserialize<'de>, T::Vector: Deserialize<'de>")
)]
struct PlayingParts<T: Animatable> {
    end: T,
    program: Program<T>,
    timing: Timing,
}

impl<T: Animatable + Serialize> Serialize for Playing<T>
where
    T::Vector: Serialize,
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut playing = serializer.serialize_struct("Playing", 3)?;
        playing.serialize_field("end", &self.end)?;
        playing.serialize_field("program", self.program())?;
        playing.serialize_field("timing", &self.timing)?;
        playing.end()
    }
}

impl<'de, T: Animatable + Deserialize<'de>> Deserialize<'de> for Playing<T>
where
    T::Vector: Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let parts = PlayingParts::<T>::deserialize(deserializer)?;
        Self::new(parts.end, parts.program, parts.timing).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests;
