// SPDX-License-Identifier: GPL-3.0-only
use super::raster::Coverage;
use crate::color::ChannelRange;
use crate::error::{Error, SkslDiagnostic};
use skia_safe::runtime_effect::ChildPtr;
use skia_safe::{Canvas, Color4f, Data, Paint, RuntimeEffect, Shader, shaders};

#[derive(Clone)]
pub(crate) struct RgbBlend {
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

    pub fn paint(
        &self,
        color: Color4f,
        drawn: Shader,
        backdrop: Shader,
        range: ChannelRange,
    ) -> Option<Paint> {
        let children = [linear_color(color), drawn, backdrop].map(ChildPtr::from);
        let clamped_to_unit_range = match range {
            ChannelRange::Unit => 1.0_f32,
            ChannelRange::Extended => 0.0,
        };
        let uniforms = Data::new_copy(&clamped_to_unit_range.to_ne_bytes());
        let blender = (self.effect).make_blender(uniforms, Some(children.as_slice()))?;
        let mut paint = Paint::default();
        let _paint_returned_for_chaining = paint.set_blender(blender);
        Some(paint)
    }

    pub fn draw(&self, canvas: &Canvas, coverage: &Coverage, color: Color4f) -> Result<(), Error> {
        let [drawn, backdrop] =
            [skia_safe::Color::WHITE, skia_safe::Color::BLACK].map(shaders::color);
        let range = ChannelRange::of(canvas.image_info().color_type());
        let mut paint =
            (self.paint(color, drawn, backdrop, range)).ok_or(Error::SubpixelBlender)?;
        let _paint_returned_for_chaining = paint.set_shader(coverage.shader.clone());
        let _canvas_returned_for_chaining = canvas.draw_rect(coverage.destination, &paint);
        Ok(())
    }
}

pub(super) fn linear_color(color: Color4f) -> Shader {
    shaders::color_in_space(color, skia_safe::ColorSpace::new_srgb_linear())
}

#[cfg(test)]
mod tests;
