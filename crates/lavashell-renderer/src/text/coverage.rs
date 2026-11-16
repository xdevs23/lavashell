// SPDX-License-Identifier: GPL-3.0-only
use crate::error::{Error, SkslDiagnostic};
use skia_safe::matrix::ScaleToFit;
use skia_safe::{Canvas, Color4f, Data, Image, Matrix, Paint, Rect, RuntimeEffect};

pub(super) struct RgbBlend {
    effect: RuntimeEffect,
}

impl RgbBlend {
    pub fn new() -> Result<Self, Error> {
        let effect = RuntimeEffect::make_for_blender(include_str!("rgb_blend.sksl"), None)
            .map_err(|diagnostic| Error::CompileSubpixelEffect {
                effect: "rgb blend",
                source: SkslDiagnostic(diagnostic),
            })?;
        Ok(Self { effect })
    }

    pub fn draw(
        &self,
        canvas: &Canvas,
        mask: &Image,
        destination: Rect,
        color: Color4f,
    ) -> Result<(), Error> {
        let foreground =
            skia_safe::shaders::color_in_space(color, skia_safe::ColorSpace::new_srgb_linear());
        let children = [skia_safe::runtime_effect::ChildPtr::from(foreground)];
        let blender = (self.effect)
            .make_blender(Data::new_empty(), Some(children.as_slice()))
            .ok_or(Error::SubpixelBlender)?;
        let shader = Matrix::rect_2_rect(
            Rect::from_irect(mask.bounds()),
            destination,
            ScaleToFit::Fill,
        )
        .and_then(|matrix| mask.to_raw_shader(None, skia_safe::SamplingOptions::default(), &matrix))
        .ok_or(Error::SubpixelMask)?;
        let mut paint = Paint::default();
        let _chained_paint = paint.set_shader(shader).set_blender(blender);
        let _chained_canvas = canvas.draw_rect(destination, &paint);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
