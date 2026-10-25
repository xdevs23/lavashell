// SPDX-License-Identifier: GPL-3.0-only
use super::{Insets, Size};
use crate::units::Dp;

/// The layout constraint on one axis: a length of at least `min` and at most `max`, with `proposed`
/// between them as the length offered to the content. Text wraps at the proposed width. A child
/// that fills is as large as the proposed length. Weighted children divide the proposed length among them. An
/// infinite `max` or `proposed` makes the axis unbounded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    min: Dp,
    max: Dp,
    proposed: Dp,
}

impl Span {
    fn new(min: Dp, max: Dp, proposed: Dp) -> Self {
        let max = max.max(min);
        Self {
            min,
            max,
            proposed: proposed.clamp(min, max),
        }
    }

    #[must_use]
    pub fn exact(length: Dp) -> Self {
        Self::new(length, length, length)
    }

    /// A span that allows any length up to `max` and proposes `max`.
    #[must_use]
    pub fn up_to(max: Dp) -> Self {
        Self::new(Dp::ZERO, max, max)
    }

    /// A span that allows any length and proposes `proposed`.
    #[must_use]
    pub fn offering(proposed: Dp) -> Self {
        Self::new(Dp::ZERO, Dp::INFINITY, proposed)
    }

    #[must_use]
    pub fn proposed(self) -> Dp {
        self.proposed
    }

    /// The proposed length, unless the axis is unbounded.
    #[must_use]
    pub fn bound(self) -> Option<Dp> {
        self.proposed.is_finite().then_some(self.proposed)
    }

    /// Whether the span allows a greater length than it proposes.
    #[must_use]
    pub fn allows_more(self) -> bool {
        self.max > self.proposed
    }

    #[must_use]
    pub fn clamp(self, length: Dp) -> Dp {
        length.clamp(self.min, self.max)
    }

    /// A span of exactly `length`, clamped into this span.
    #[must_use]
    pub fn at(self, length: Dp) -> Self {
        Self::exact(self.clamp(length))
    }

    #[must_use]
    pub fn at_most(self, length: Dp) -> Self {
        Self::new(self.min, length.min(self.max), length.min(self.proposed))
    }

    #[must_use]
    pub fn at_least(self, length: Dp) -> Self {
        let min = length.max(self.min).min(self.max);
        Self::new(min, self.max, self.proposed.max(min))
    }

    /// The span for a child that fills this span. Its minimum is the proposed length, or the minimum of this span
    /// when the axis is unbounded.
    #[must_use]
    pub fn filled(self) -> Self {
        Self::new(
            self.bound().map_or(self.min, |bound| bound.max(self.min)),
            self.max,
            self.proposed,
        )
    }

    /// This span without its minimum.
    #[must_use]
    pub fn loosened(self) -> Self {
        Self::new(Dp::ZERO, self.max, self.proposed)
    }

    #[must_use]
    pub fn unbounded(self) -> Self {
        Self::new(self.min, Dp::INFINITY, Dp::INFINITY)
    }

    /// A span that allows any length up to `limit` when this span is bounded, and any length when it is unbounded.
    #[must_use]
    pub fn limited(self, limit: Dp) -> Self {
        Self::up_to(self.bound().map_or(Dp::INFINITY, |_| limit))
    }

    /// This span shortened by `length` at each bound.
    #[must_use]
    pub fn shrunk(self, length: Dp) -> Self {
        let shrink = |bound: Dp| (bound - length).max(Dp::ZERO);
        Self::new(shrink(self.min), shrink(self.max), shrink(self.proposed))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Constraints {
    pub width: Span,
    pub height: Span,
}

impl Constraints {
    #[must_use]
    pub fn tight(size: Size) -> Self {
        Self {
            width: Span::exact(size.width),
            height: Span::exact(size.height),
        }
    }

    #[must_use]
    pub fn constrain(self, size: Size) -> Size {
        Size::new(self.width.clamp(size.width), self.height.clamp(size.height))
    }

    #[must_use]
    pub fn shrink(self, i: Insets) -> Self {
        Self {
            width: self.width.shrunk(i.horizontal()),
            height: self.height.shrunk(i.vertical()),
        }
    }
}
