// SPDX-License-Identifier: GPL-3.0-only
use super::{Corners, Hollow, Polygon, RoundedRect};
use crate::geometry::{Point, Rect};
use crate::units::{Dp, NonNegativeDp};

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

    #[must_use]
    pub fn members(&self) -> &[UnionMember] {
        &self.members
    }

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
