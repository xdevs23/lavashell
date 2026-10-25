// SPDX-License-Identifier: GPL-3.0-only
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Destination {
    Surface,
    Layer,
}

impl Destination {
    pub fn faded(opacity: f64) -> Self {
        if opacity >= 1.0 {
            Self::Surface
        } else {
            Self::Layer
        }
    }

    /// The destination of painting inside a group that is opened while painting into `self` and that paints
    /// into `group`. Painting inside a layer always goes into a layer.
    pub fn enter(self, group: Self) -> Self {
        match group {
            Self::Surface => self,
            Self::Layer => Self::Layer,
        }
    }
}
