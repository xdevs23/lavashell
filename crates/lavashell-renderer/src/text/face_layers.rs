// SPDX-License-Identifier: GPL-3.0-only
use super::coverage::linear_color;
use super::shaper::FaceShare;
use super::{SavedScope, Scopes, TextBlends};
use crate::color::ChannelRange;
use crate::destination::Destination;
use crate::failure::DrawFailure;
use crate::scalar::scalar;
use lavashell_core::prelude::*;
use skia_safe::canvas::{SaveLayerFlags, SaveLayerRec};
use skia_safe::{Canvas, Paint, Rect, Shader};

pub(super) enum FaceLayers {
    ColorInLayer,
    LcdCoverage {
        backdrop: Shader,
        applying_color: Paint,
    },
}

impl FaceLayers {
    pub fn lcd_coverage(
        blends: &TextBlends,
        color: Color,
        range: ChannelRange,
    ) -> Result<Self, DrawFailure> {
        let drawn = linear_color(crate::paint::color4f(color.with_alpha(1.0))?);
        let backdrop = (blends.face_backdrop).opposite_of(drawn.clone())?;
        let applying_color = (blends.rgb)
            .paint(
                crate::paint::color4f(color)?,
                drawn,
                backdrop.clone(),
                range,
            )
            .ok_or(DrawFailure::ShaderRejected)?;
        Ok(Self::LcdCoverage {
            backdrop,
            applying_color,
        })
    }

    pub fn style_for_face_drawing(&self, face: &FaceShare, style: &TextStyle<Dp>) -> TextStyle<Dp> {
        match self {
            Self::ColorInLayer => face.faded_style(style),
            Self::LcdCoverage { .. } => TextStyle {
                weight: face.weight,
                color: style.color.with_alpha(1.0),
                ..style.clone()
            },
        }
    }

    pub fn paint_for_first_face_layer(&self) -> Paint {
        match self {
            Self::ColorInLayer => Paint::default(),
            Self::LcdCoverage { applying_color, .. } => applying_color.clone(),
        }
    }

    pub fn paint_for_second_face_layer(
        &self,
        blends: &TextBlends,
        [lighter, heavier]: &[FaceShare; 2],
    ) -> Result<Paint, DrawFailure> {
        let shares = match self {
            Self::ColorInLayer => [1.0, 1.0],
            Self::LcdCoverage { .. } => [
                scalar(lighter.share_of_coverage)?,
                scalar(heavier.share_of_coverage)?,
            ],
        };
        blends.coverage_sum.paint(self.backdrop(), shares)
    }

    pub fn save<'c>(
        &self,
        scopes: &Scopes,
        canvas: &'c Canvas,
        bounds: &Rect,
        restore: &Paint,
    ) -> SavedScope<'c> {
        let (destination, flags) = match self {
            Self::ColorInLayer => (Destination::Layer, SaveLayerFlags::empty()),
            Self::LcdCoverage { .. } => (Destination::Surface, SaveLayerFlags::PRESERVE_LCD_TEXT),
        };
        let saved = scopes.save(canvas, destination);
        let layer = SaveLayerRec::default()
            .bounds(bounds)
            .paint(restore)
            .flags(flags);
        let _restored_by_saved_scope = canvas.save_layer(&layer);
        let mut clearing = Paint::default();
        let _paint_returned_for_chaining = clearing
            .set_shader(self.backdrop())
            .set_blend_mode(skia_safe::BlendMode::Src);
        let _canvas_returned_for_chaining = canvas.draw_paint(&clearing);
        saved
    }

    fn backdrop(&self) -> Shader {
        match self {
            Self::ColorInLayer => skia_safe::shaders::color(skia_safe::Color::TRANSPARENT),
            Self::LcdCoverage { backdrop, .. } => backdrop.clone(),
        }
    }
}
