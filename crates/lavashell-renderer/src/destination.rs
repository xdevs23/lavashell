// SPDX-License-Identifier: GPL-3.0-only
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Destination {
    Surface,
    Layer,
}

impl Destination {
    pub fn inside(self, group: Self) -> Self {
        match group {
            Self::Surface => self,
            Self::Layer => Self::Layer,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GroupOpacity {
    Opaque,
    Translucent { alpha: f64 },
}

impl GroupOpacity {
    pub fn new(alpha: f64) -> Self {
        if alpha >= 1.0 {
            Self::Opaque
        } else {
            Self::Translucent { alpha }
        }
    }

    pub fn destination(self) -> Destination {
        match self {
            Self::Opaque => Destination::Surface,
            Self::Translucent { .. } => Destination::Layer,
        }
    }
}
