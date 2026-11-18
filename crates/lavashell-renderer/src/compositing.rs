// SPDX-License-Identifier: GPL-3.0-only

use crate::Painter;
use crate::destination::Destination;
use skia_safe::{
    Canvas, ColorSpace, Paint,
    canvas::{SaveLayerFlags, SaveLayerRec},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlendSpace {
    #[default]
    Linear,
    Srgb,
}

impl Painter {
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
            let _paint_returned_for_chaining = paint.set_blend_mode(skia_safe::BlendMode::Src);
            let _restored_at_the_end_of_the_frame = canvas.save_layer(
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
            let mut painter = Painter::without_device(&crate::Fonts::new())?;
            painter.set_blend_space(space);
            let info = crate::color::working_image_info((4, 4))?;
            let mut surface =
                skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
            let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::BLACK);
            painter.paint_as_leaf(
                surface.canvas(),
                &[DrawCommand::Fill {
                    shape: Rect::new(0.dp(), 0.dp(), 4.dp(), 4.dp()).into(),
                    brush: Color::WHITE.with_alpha(0.5).into(),
                }],
                Density::ONE,
            )?;
            let pixel = crate::test_surfaces::pixels(&mut surface).at(1, 1);
            assert!(pixel[0].abs_diff(expected) <= 1, "{space:?}: {pixel:?}");
            assert_eq!(pixel[3], 255);
        }
        Ok(())
    }
}
