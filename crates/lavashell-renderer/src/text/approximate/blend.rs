// SPDX-License-Identifier: GPL-3.0-only
use super::super::raster::{Coverage, sampled_mask};
use super::masks::Masks;
use crate::color::{HALF_EPSILON, channel_step};
use crate::error::{Error, SkslDiagnostic};
use crate::scalar::scalar;
use skia_safe::{Canvas, Color4f, Data, Paint, RuntimeEffect};

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

    pub fn coverage(&self, masks: &Masks) -> Result<Coverage, Error> {
        let sampled = |mask| sampled_mask(mask, masks.destination);
        let children: [skia_safe::runtime_effect::ChildPtr; 2] =
            [sampled(&masks.rgb)?.into(), sampled(&masks.center)?.into()];
        let shader = (self.source)
            .make_shader(Data::new_empty(), &children, None)
            .ok_or(Error::SubpixelMask)?;
        Ok(Coverage {
            shader,
            destination: masks.destination,
        })
    }

    pub fn draw(&self, canvas: &Canvas, coverage: &Coverage, color: Color4f) -> Result<(), Error> {
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
        let _paint_returned_for_chaining = paint
            .set_shader(coverage.shader.clone())
            .set_blender(blender);
        let _canvas_returned_for_chaining = canvas.draw_rect(coverage.destination, &paint);
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
