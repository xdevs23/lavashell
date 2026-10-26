// SPDX-License-Identifier: GPL-3.0-only
use super::{Corners, Hollow, Polygon, RoundedRect};
use crate::geometry::{Point, Rect};
use crate::units::{Dp, NonNegativeDp};

/// A union of shapes. Members merge into one outline wherever the gap between them is at most
/// `spacing`. The merged outline lies inside the bounding box of all members.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(try_from = "super::encoding::UnionParts")
)]
pub struct Union {
    members: Vec<UnionMember>,
    spacing: NonNegativeDp,
    #[cfg_attr(feature = "serde", serde(skip_serializing))]
    bounds: Rect,
}

impl Union {
    /// `None` without members.
    pub(super) fn new(members: Vec<UnionMember>, spacing: NonNegativeDp) -> Option<Self> {
        let bounds = members
            .iter()
            .map(UnionMember::bounds)
            .reduce(Rect::union)?;
        Some(Self {
            members,
            spacing,
            bounds,
        })
    }

    /// The member shapes of the union. A union has at least one member.
    #[must_use]
    pub fn members(&self) -> &[UnionMember] {
        &self.members
    }

    /// The largest gap at which members fuse.
    #[must_use]
    pub fn spacing(&self) -> Dp {
        self.spacing.get()
    }

    #[must_use]
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    pub(super) fn translated(&self, offset: Point) -> Self {
        Self {
            members: (self.members.iter())
                .map(|member| member.translated(offset))
                .collect(),
            spacing: self.spacing,
            bounds: self.bounds.translated(offset),
        }
    }

    /// This union with the hole of every member filled, `None` when no member has a hole.
    pub(super) fn without_holes(&self) -> Option<Self> {
        let hollow = |member: &UnionMember| matches!(member, UnionMember::Hollow(_));
        self.members.iter().any(hollow).then(|| Self {
            members: (self.members.iter())
                .map(|member| match member {
                    UnionMember::Hollow(shape) => UnionMember::Rounded(shape.outer),
                    UnionMember::Rounded(_) | UnionMember::Contour(_) => member.clone(),
                })
                .collect(),
            spacing: self.spacing,
            bounds: self.bounds,
        })
    }
}

/// A shape that a union merges, which is never a union itself.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum UnionMember {
    Rounded(RoundedRect),
    Contour(Polygon),
    Hollow(Hollow),
}

impl From<RoundedRect> for UnionMember {
    fn from(shape: RoundedRect) -> Self {
        Self::Rounded(shape)
    }
}

impl From<Rect> for UnionMember {
    fn from(rect: Rect) -> Self {
        Self::Rounded(RoundedRect::new(rect, Corners::default()))
    }
}

impl UnionMember {
    /// The bounds of the outline, which for a member with a hole are the bounds of its outer rectangle.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        match self {
            Self::Rounded(shape) => shape.rect,
            Self::Contour(shape) => shape.bounds(),
            Self::Hollow(shape) => shape.outer.rect,
        }
    }

    fn translated(&self, offset: Point) -> Self {
        match self {
            Self::Rounded(shape) => Self::Rounded(shape.translated(offset)),
            Self::Contour(shape) => Self::Contour(shape.translated(offset)),
            Self::Hollow(shape) => Self::Hollow(shape.translated(offset)),
        }
    }
}
