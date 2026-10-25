// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::Size;
use lavashell_core::units::NonNegativeDp;
use serde::{Deserialize, Serialize};

/// A size whose width and height are finite and at least zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NonNegativeSize {
    pub width: NonNegativeDp,
    pub height: NonNegativeDp,
}

impl NonNegativeSize {
    #[must_use]
    pub const fn get(self) -> Size {
        Size::new(self.width.get(), self.height.get())
    }
}
