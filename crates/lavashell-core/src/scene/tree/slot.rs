// SPDX-License-Identifier: GPL-3.0-only
use super::{Presence, PresenceMap, SceneTree};
use crate::animation::{Animatable, Playing};
use crate::geometry::{Axis, Point};
use crate::scene::Affine;
use crate::shape::RoundedRect;
use crate::units::Dp;
use std::time::Duration;

#[derive(Clone, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(bound(
        serialize = "T: serde::Serialize, T::Motion: serde::Serialize",
        deserialize = "T: serde::Deserialize<'de>, T::Motion: serde::Deserialize<'de>"
    ))
)]
pub enum Slot<T: SlotValue> {
    Value(T),
    Motion(Box<T::Motion>),
}

impl<T: SlotValue> Slot<T> {
    pub fn at(&self, now: Duration) -> T {
        match self {
            Self::Value(value) => value.clone(),
            Self::Motion(motion) => motion.at(now),
        }
    }

    fn next_change(&self, now: Duration) -> Option<Duration> {
        match self {
            Self::Value(_) => None,
            Self::Motion(motion) => motion.next_change(now),
        }
    }

    pub(crate) fn moves(&self) -> bool {
        matches!(self, Self::Motion(_))
    }

    pub(crate) fn end(&self) -> T {
        match self {
            Self::Value(value) => value.clone(),
            Self::Motion(motion) => motion.end(),
        }
    }
}

mod sealed {
    pub trait Sealed {}
}

pub trait SlotValue: sealed::Sealed + Clone + PartialEq + 'static {
    type Motion: SlotMotion<Self>;
}

pub trait SlotMotion<T>: sealed::Sealed + Clone + PartialEq {
    fn at(&self, now: Duration) -> T;

    fn end(&self) -> T;

    fn next_change(&self, now: Duration) -> Option<Duration>;
}

impl<T: Animatable> SlotMotion<T> for Playing<T> {
    fn at(&self, now: Duration) -> T {
        Playing::at(self, now)
    }

    fn end(&self) -> T {
        Playing::end(self).clone()
    }

    fn next_change(&self, now: Duration) -> Option<Duration> {
        Playing::next_change(self, now)
    }
}

impl<T: Animatable> sealed::Sealed for Playing<T> {}

impl sealed::Sealed for TransformMotion {}

impl sealed::Sealed for f64 {}

impl sealed::Sealed for RoundedRect {}

impl sealed::Sealed for Affine {}

impl SlotValue for f64 {
    type Motion = Playing<f64>;
}

impl SlotValue for RoundedRect {
    type Motion = Playing<RoundedRect>;
}

impl SlotValue for Affine {
    type Motion = TransformMotion;
}

#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TransformMotion {
    Affine(Playing<Affine>),
    Clamp {
        offset: Playing<Dp>,
        clamp: Clamp,
    },
    Presence {
        presence: Playing<(Point, Presence)>,
        map: PresenceMap,
    },
}

impl SlotMotion<Affine> for TransformMotion {
    fn at(&self, now: Duration) -> Affine {
        match self {
            Self::Affine(playing) => playing.at(now),
            Self::Clamp { offset, clamp } => clamp.affine(offset.at(now)),
            Self::Presence { presence, map } => {
                let (position, presence) = presence.at(now);
                map.transform(position, &presence).affine()
            }
        }
    }

    fn end(&self) -> Affine {
        match self {
            Self::Affine(playing) => *playing.end(),
            Self::Clamp { offset, clamp } => clamp.affine(*offset.end()),
            Self::Presence { presence, map } => {
                let (position, presence) = presence.end();
                map.transform(*position, presence).affine()
            }
        }
    }

    fn next_change(&self, now: Duration) -> Option<Duration> {
        match self {
            Self::Affine(playing) => playing.next_change(now),
            Self::Clamp { offset, .. } => offset.next_change(now),
            Self::Presence { presence, .. } => presence.next_change(now),
        }
    }
}

/// A translation along `axis` by an offset clamped to `min` and `max`, then multiplied by `scale`. When `min` lies
/// above `max`, the offset becomes `max`. A bound that is NaN does not limit the offset.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Clamp {
    pub min: Dp,
    pub max: Dp,
    pub axis: Axis,
    pub scale: f64,
}

impl Clamp {
    pub(crate) fn translation(self, offset: Dp) -> Point {
        self.axis
            .vector(offset.max(self.min).min(self.max) * self.scale)
    }

    pub(crate) fn affine(self, offset: Dp) -> Affine {
        Affine::translate(self.translation(offset))
    }
}

#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Group {
    /// Maps the coordinates of the children into those of the parent.
    pub transform: Slot<Affine>,
    /// An opacity below 0 paints as 0, and one above 1 paints as 1.
    pub opacity: Slot<f64>,
    pub clip: Option<Slot<RoundedRect>>,
}

impl Default for Group {
    fn default() -> Self {
        GroupValues::default().into()
    }
}

impl Group {
    #[must_use]
    pub fn at(&self, now: Duration) -> GroupValues {
        GroupValues {
            transform: self.transform.at(now),
            opacity: self.opacity.at(now),
            clip: self.clip.as_ref().map(|clip| clip.at(now)),
        }
    }

    pub(crate) fn end(&self) -> GroupValues {
        GroupValues {
            transform: self.transform.end(),
            opacity: self.opacity.end(),
            clip: self.clip.as_ref().map(Slot::end),
        }
    }

    pub(crate) fn moves(&self) -> bool {
        self.transform.moves()
            || self.opacity.moves()
            || self.clip.as_ref().is_some_and(Slot::moves)
    }

    fn next_change(&self, now: Duration) -> Option<Duration> {
        let clip = self.clip.as_ref().and_then(|clip| clip.next_change(now));
        [
            self.transform.next_change(now),
            self.opacity.next_change(now),
            clip,
        ]
        .into_iter()
        .flatten()
        .min()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupValues {
    pub transform: Affine,
    pub opacity: f64,
    pub clip: Option<RoundedRect>,
}

impl Default for GroupValues {
    fn default() -> Self {
        Self {
            transform: Affine::IDENTITY,
            opacity: 1.0,
            clip: None,
        }
    }
}

impl From<GroupValues> for Group {
    fn from(values: GroupValues) -> Self {
        Self {
            transform: Slot::Value(values.transform),
            opacity: Slot::Value(values.opacity),
            clip: values.clip.map(Slot::Value),
        }
    }
}

impl SceneTree {
    /// The earliest time, at or after `now`, at which an animated group of the tree changes, or `None` when every
    /// animation is paused or at rest. From that time on, a consumer presents the tree on every frame.
    #[must_use]
    pub fn next_change(&self, now: Duration) -> Option<Duration> {
        (self.moving.iter())
            .filter_map(|key| self.group(*key)?.next_change(now))
            .min()
    }
}
