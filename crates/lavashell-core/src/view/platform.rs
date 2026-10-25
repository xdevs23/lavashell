// SPDX-License-Identifier: GPL-3.0-only
use super::{Dp, PlatformFacts, View};
use crate::platform::{CARET_WIDTH, TOUCH_SLOP};
use crate::units::PxExt;

impl View {
    /// Replaces the platform facts of the view. A host calls this whenever it reads new facts from the platform. Only
    /// the parts of the view that read a changed fact compose again.
    pub fn provide_platform(&self, platform: PlatformFacts) {
        self.root_values.set_platform(platform);
    }

    /// The platform facts last passed to the view, such as the touch slop and the caret width.
    #[must_use]
    pub fn platform(&self) -> PlatformFacts {
        self.root_values.platform()
    }

    /// The length of one physical pixel of the surface of the view.
    pub(super) fn device_pixel(&self) -> Dp {
        self.density.get().to_dp(1.px())
    }

    /// How far a pressed pointer moves before it drags, scrolls, pinches or selects text. Without a value from the
    /// platform, this is one device pixel.
    pub(super) fn touch_slop(&self) -> Dp {
        (self.root_values.value(&TOUCH_SLOP).get()).unwrap_or_else(|| self.device_pixel())
    }

    /// The width of the caret of a text field. Without a value from the platform, this is one device pixel.
    pub(super) fn caret_width(&self) -> Dp {
        (self.root_values.value(&CARET_WIDTH).get()).unwrap_or_else(|| self.device_pixel())
    }
}
