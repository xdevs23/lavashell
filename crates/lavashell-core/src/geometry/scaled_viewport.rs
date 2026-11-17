// SPDX-License-Identifier: GPL-3.0-only
use super::{PixelScale, PixelSize, Viewport};
use crate::units::NumberError;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaledViewport {
    pub viewport: Viewport,
    pub scale: PixelScale,
}

impl ScaledViewport {
    pub fn buffer_size(self) -> Result<PixelSize, NumberError> {
        self.scale.buffer_size(self.viewport)
    }
}
