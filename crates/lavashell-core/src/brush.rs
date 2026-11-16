// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::{Color, Point};
use crate::gradient::Gradient;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Brush {
    Solid(Color),
    Gradient(Gradient),
}

impl From<Gradient> for Brush {
    fn from(g: Gradient) -> Self {
        Self::Gradient(g)
    }
}

impl Brush {
    #[must_use]
    pub fn translated(self, offset: Point) -> Self {
        match self {
            Self::Solid(c) => Self::Solid(c),
            Self::Gradient(g) => Self::Gradient(g.translated(offset)),
        }
    }
}

impl From<Color> for Brush {
    fn from(c: Color) -> Self {
        Self::Solid(c)
    }
}
