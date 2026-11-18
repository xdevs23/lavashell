// SPDX-License-Identifier: GPL-3.0-only
use super::View;
use crate::platform::{CARET_WIDTH, PlatformFacts, SLOP};
use crate::units::{Dp, PxExt};

impl View {
    pub fn provide_platform(&self, platform: PlatformFacts) {
        self.root_values.set_platform(platform);
    }

    #[must_use]
    pub fn platform(&self) -> PlatformFacts {
        self.root_values.platform()
    }

    pub(super) fn device_pixel(&self) -> Dp {
        self.density.get().to_dp(1.px())
    }

    pub(super) fn slop(&self) -> Dp {
        (self.root_values.value(&SLOP).get()).unwrap_or_else(|| self.device_pixel())
    }

    pub(super) fn caret_width(&self) -> Dp {
        (self.root_values.value(&CARET_WIDTH).get()).unwrap_or_else(|| self.device_pixel())
    }
}
