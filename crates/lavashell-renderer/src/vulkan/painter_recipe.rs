// SPDX-License-Identifier: GPL-3.0-only
use super::skia_handoff::Glyphs;
use crate::{Error, Fonts, Painter, UniformRange};
use lavashell_vulkan::PhysicalDevice;

pub(super) struct PainterRecipe {
    uniform_range: UniformRange,
    glyphs: Glyphs,
}

impl PainterRecipe {
    pub fn new(physical: &PhysicalDevice<'_>, glyphs: Glyphs) -> Self {
        let limits = physical.properties().limits;
        Self {
            uniform_range: UniformRange::of_device(limits.max_uniform_buffer_range),
            glyphs,
        }
    }

    pub fn painter(&self, fonts: &Fonts) -> Result<Painter, Error> {
        let painter = Painter::new(fonts, self.uniform_range)?;
        Ok(painter.with_raster_mode(self.glyphs.raster_mode()))
    }
}
