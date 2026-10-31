// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::Size;
use lavashell_core::scene::TextStyle;
use lavashell_core::units::Dp;
use lavashell_core::view::TextMeasurer;

/// A text measurer that returns a zero size for every text.
pub struct NoText;

impl TextMeasurer for NoText {
    fn measure(&mut self, _: &str, _: &TextStyle<Dp>, _: Dp) -> Size {
        Size::ZERO
    }
}
