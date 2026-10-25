// SPDX-License-Identifier: GPL-3.0-only
use crate::error::{Error, SkslDiagnostic};
use skia_safe::matrix::ScaleToFit;
use skia_safe::{Canvas, Color4f, Data, Image, Matrix, Paint, Rect, RuntimeEffect};

/// The Skia blender that draws an RGB subpixel mask in the foreground color over an opaque destination. The mask is
/// sampled without color conversion, because its samples are coverage values and converting them as colors would
/// change the opacity of the glyph edges. Only the foreground color is converted from linear sRGB into the color
/// space of the canvas.
pub(super) struct RgbBlend {
    effect: RuntimeEffect,
}

impl RgbBlend {
    pub fn new() -> Result<Self, Error> {
        let effect = RuntimeEffect::make_for_blender(
            "uniform shader foreground; half4 main(half4 src, half4 dst) {\
             half4 color = foreground.eval(float2(0));\
             return half4(src.rgb * color.rgb + (1 - src.rgb * color.a) * dst.rgb, 1); }",
            None,
        )
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
