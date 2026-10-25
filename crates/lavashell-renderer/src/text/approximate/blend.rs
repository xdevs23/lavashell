// SPDX-License-Identifier: GPL-3.0-only
use super::masks::Masks;
use crate::color::{HALF_EPSILON, channel_step};
use crate::error::{Error, SkslDiagnostic};
use crate::scalar::scalar;
use skia_safe::matrix::ScaleToFit;
use skia_safe::{Canvas, Color4f, Data, Matrix, Paint, Rect, RuntimeEffect, SamplingOptions};

/// The Skia effects that draw the subpixel masks of a paragraph in the foreground color, with a separate coverage
/// for each color channel. The masks are sampled without color conversion, because their samples are coverage values
/// and not colors. Only the foreground color is converted from linear sRGB into the color space of the canvas.
pub(super) struct MaskBlend {
    source: RuntimeEffect,
    blender: RuntimeEffect,
}

impl MaskBlend {
    pub fn new() -> Result<Self, Error> {
        let source = RuntimeEffect::make_for_shader(
            "uniform shader rgb; uniform shader center; half4 main(float2 p) {\
             return half4(rgb.eval(p).rgb, center.eval(p).a); }",
            None,
        )
        .map_err(|diagnostic| Error::CompileSubpixelEffect {
            effect: "mask",
            source: SkslDiagnostic(diagnostic),
        })?;
        let blender = RuntimeEffect::make_for_blender(
            "uniform shader foreground; uniform float tolerance; half4 main(half4 src, half4 dst) {\
             half3 f = foreground.eval(float2(0)).rgb;\
             half t = half(tolerance);\
             half a = src.a + (1 - src.a) * dst.a;\
             if (any(lessThan(dst.rgb, half3(-t))) || any(greaterThan(dst.rgb, half3(dst.a + t))) ||\
                 any(lessThan(f, half3(-t))) || any(greaterThan(f, half3(1 + t)))) {\
                 return half4(src.a * f + (1 - src.a) * dst.rgb, a); }\
             f = clamp(f, half3(0), half3(1));\
             half3 rgb = src.rgb * f + (1 - src.rgb) * dst.rgb;\
             return half4(rgb, min(1, max(a, max(max(rgb.r, rgb.g), rgb.b)))); }",
            None,
        )
        .map_err(|diagnostic| Error::CompileSubpixelEffect {
            effect: "mask blend",
            source: SkslDiagnostic(diagnostic),
        })?;
        Ok(Self { source, blender })
    }

    pub fn draw(&self, canvas: &Canvas, masks: &Masks, color: Color4f) -> Result<(), Error> {
        let rect = masks.destination;
        let sampled = |mask: &skia_safe::Image| {
            Matrix::rect_2_rect(Rect::from_irect(mask.bounds()), rect, ScaleToFit::Fill)
                .and_then(|matrix| mask.to_raw_shader(None, SamplingOptions::default(), &matrix))
                .ok_or(Error::SubpixelMask)
        };
        let children: [skia_safe::runtime_effect::ChildPtr; 2] =
            [sampled(&masks.rgb)?.into(), sampled(&masks.center)?.into()];
        let shader = (self.source)
            .make_shader(Data::new_empty(), &children, None)
            .ok_or(Error::SubpixelMask)?;
        let foreground =
            skia_safe::shaders::color_in_space(color, skia_safe::ColorSpace::new_srgb_linear());
        let children: [skia_safe::runtime_effect::ChildPtr; 1] = [foreground.into()];
        // The blender computes in half floats, which cannot tell apart two values closer than `HALF_EPSILON`. The
        // tolerance for out-of-range colors is therefore at least that step.
        let tolerance = channel_step(canvas.image_info().color_type())
            .map_or(HALF_EPSILON, |step| step.max(HALF_EPSILON));
        let tolerance = scalar(tolerance).map_err(Error::SubpixelBlendTolerance)?;
        let uniforms = Data::new_copy(&tolerance.to_ne_bytes());
        let blender = (self.blender)
            .make_blender(uniforms, Some(children.as_slice()))
            .ok_or(Error::SubpixelBlender)?;
        let mut paint = Paint::default();
        let _chained_paint = paint.set_shader(shader).set_blender(blender);
        let _chained_canvas = canvas.draw_rect(rect, &paint);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::MaskBlend;

    #[test]
    fn coverage_runtime_effects_compile() {
        let compiled = MaskBlend::new();
        assert!(compiled.is_ok(), "{:?}", compiled.err());
    }
}
