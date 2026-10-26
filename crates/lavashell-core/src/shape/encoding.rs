// SPDX-License-Identifier: GPL-3.0-only
//! Shapes are read through the checks of their constructors. The members of a union are never unions, so reading a
//! shape never recurses.

use super::{Hollow, Polygon, RoundedRect, Union, UnionMember};
use crate::encoding::sequence;
use crate::geometry::Point;
use crate::units::NonNegativeDp;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

#[derive(Deserialize)]
#[serde(rename = "Hollow")]
pub(super) struct HollowParts {
    outer: RoundedRect,
    hole: RoundedRect,
}

#[derive(Deserialize)]
#[serde(rename = "Union")]
pub(super) struct UnionParts {
    #[serde(deserialize_with = "sequence")]
    members: Vec<UnionMember>,
    spacing: NonNegativeDp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum ShapeError {
    #[error("a union needs at least one member")]
    UnionWithoutMembers,
    #[error("the hole of a hollow shape needs a positive width and height")]
    HoleWithoutArea,
}

impl TryFrom<HollowParts> for Hollow {
    type Error = ShapeError;

    fn try_from(HollowParts { outer, hole }: HollowParts) -> Result<Self, ShapeError> {
        Self::new(outer, hole).ok_or(ShapeError::HoleWithoutArea)
    }
}

impl TryFrom<UnionParts> for Union {
    type Error = ShapeError;

    fn try_from(UnionParts { members, spacing }: UnionParts) -> Result<Self, ShapeError> {
        Self::new(members, spacing).ok_or(ShapeError::UnionWithoutMembers)
    }
}

impl Serialize for Polygon {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.points.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Polygon {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let points: Vec<Point> = sequence(deserializer)?;
        Self::new(points).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests;
