// SPDX-License-Identifier: GPL-3.0-only
//! The choice of the color space in which the painter blends colors. Linear light is the default. Blending in
//! gamma-encoded sRGB is available for an app whose artwork was designed with sRGB blending and must match it.

use crate::Painter;
use crate::destination::Destination;
use skia_safe::{
    Canvas, ColorSpace, Paint,
    canvas::{SaveLayerFlags, SaveLayerRec},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlendSpace {
    /// Blends colors in linear light in the working color space, linear Rec.2020, which can represent wide-gamut
    /// and HDR colors.
    #[default]
    Linear,
    /// Blends colors in gamma-encoded sRGB. The painter opens one sRGB layer over the whole surface for this.
    Srgb,
}

impl Painter {
    /// Sets the color space in which colors are blended. After a change, every surface is painted again in full
    /// at its next paint, even when its draw commands did not change.
    pub fn set_blend_space(&mut self, space: BlendSpace) {
        if self.blend_space != space {
            self.invalidate_paint_configuration();
            self.blend_space = space;
        }
    }

    pub(crate) fn compositing_destination(&self) -> Destination {
        match self.blend_space {
            BlendSpace::Linear => Destination::Surface,
            BlendSpace::Srgb => Destination::Layer,
        }
    }

    pub(crate) fn begin_compositing(&self, canvas: &Canvas) {
        if self.compositing_destination() == Destination::Layer {
            let color_space = ColorSpace::new_srgb();
            let mut paint = Paint::default();
            let _chained_paint = paint.set_blend_mode(skia_safe::BlendMode::Src);
            let _save_count = canvas.save_layer(
                &SaveLayerRec::default()
                    .color_space(&color_space)
                    .paint(&paint)
                    .flags(SaveLayerFlags::INIT_WITH_PREVIOUS),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;
    use lavashell_core::prelude::*;

    #[test]
    fn explicit_srgb_blending_preserves_the_default_linear_result() -> Result<(), TestError> {
        for (space, expected) in [(BlendSpace::Linear, 188u8), (BlendSpace::Srgb, 128u8)] {
            let mut painter =
                Painter::new(&crate::Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
            painter.set_blend_space(space);
            let info = skia_safe::ImageInfo::new(
                (4, 4),
                skia_safe::ColorType::RGBAF16,
                skia_safe::AlphaType::Premul,
                crate::color::working_color_space().ok_or(crate::Error::WorkingColorSpace)?,
            );
            let mut surface =
                skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
            let _chained_canvas = surface.canvas().clear(skia_safe::Color::BLACK);
            painter.paint_as_leaf(
                surface.canvas(),
                &[DrawCommand::Fill {
                    shape: Rect::new(0.dp(), 0.dp(), 4.dp(), 4.dp()).into(),
                    brush: Color::WHITE.with_alpha(0.5).into(),
                }],
                Density::ONE,
            )?;
            let info = skia_safe::ImageInfo::new(
                (1, 1),
                skia_safe::ColorType::RGBA8888,
                skia_safe::AlphaType::Unpremul,
                ColorSpace::new_srgb(),
            );
            let mut pixel = [0u8; 4];
            assert!(surface.read_pixels(&info, &mut pixel, 4, (1, 1)));
            assert!(pixel[0].abs_diff(expected) <= 1, "{space:?}: {pixel:?}");
            assert_eq!(pixel[3], 255);
        }
        Ok(())
    }
}
