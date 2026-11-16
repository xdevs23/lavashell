// SPDX-License-Identifier: GPL-3.0-only
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Destination {
    Surface,
    Layer,
}

impl Destination {
    pub fn for_opacity(opacity: f64) -> Self {
        if opacity >= 1.0 {
            Self::Surface
        } else {
            Self::Layer
        }
    }

    pub fn inside(self, group: Self) -> Self {
        match group {
            Self::Surface => self,
            Self::Layer => Self::Layer,
        }
    }
}
