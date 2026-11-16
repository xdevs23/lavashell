// SPDX-License-Identifier: GPL-3.0-only
use super::masks::Masks;
use crate::color::{HALF_EPSILON, channel_step};
use crate::error::{Error, SkslDiagnostic};
use crate::scalar::scalar;
use skia_safe::matrix::ScaleToFit;
use skia_safe::{Canvas, Color4f, Data, Matrix, Paint, Rect, RuntimeEffect, SamplingOptions};

pub(super) struct MaskBlend {
    source: RuntimeEffect,
    blender: RuntimeEffect,
}

impl MaskBlend {
    pub fn new() -> Result<Self, Error> {
        let source = RuntimeEffect::make_for_shader(include_str!("mask_source.sksl"), None)
            .map_err(|diagnostic| Error::CompileSubpixelEffect {
                effect: "mask",
                source: SkslDiagnostic(diagnostic),
            })?;
        let blender = RuntimeEffect::make_for_blender(include_str!("mask_blend.sksl"), None)
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
        let step =
            channel_step(canvas.image_info().color_type()).ok_or(Error::SubpixelColorType)?;
        let tolerance = step.max(HALF_EPSILON);
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
