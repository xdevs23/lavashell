// SPDX-License-Identifier: GPL-3.0-only
mod approximate;
mod blends;
mod coverage;
mod coverage_sum;
mod face_backdrop;
mod face_layers;
mod glyphs;
mod lcd;
mod paintable;
mod policy;
mod raster;
mod scope;
mod shaper;

use crate::color::ChannelRange;
use crate::failure::{DrawFailure, Failures};
use crate::paint::{device_scale, dp_of};
use crate::raster::{RasterCache, outset_by_antialiasing};
use crate::scalar::{scalar, scalars};
use crate::text_cache::ShapedParagraph;
pub(crate) use blends::TextBlends;
use face_layers::FaceLayers;
pub(crate) use glyphs::Glyphs;
use lavashell_core::scene::TextStyle;
use lavashell_core::units::Dp;
use paintable::{Face, ParagraphDrawer};
pub(crate) use paintable::{FaceParagraphs, PaintableParagraph, PaintableText, ParagraphRequest};
pub use policy::{Hinting, PixelGeometry, TextAntialiasing, TextDestination, TextRendering};
pub(crate) use raster::{Coverage, Drawn, Rasterizers};
#[cfg(test)]
pub(crate) use raster::{RasterInput, TextRasterizer};
pub(crate) use scope::{SavedScope, Scopes};
pub use shaper::Shaper;
use shaper::{FaceShare, WeightRendering};
use skia_safe::textlayout::Paragraph;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Configuration {
    pub rendering: TextRendering,
    pub destination: TextDestination,
}

impl crate::Painter {
    pub fn set_text_rendering(&mut self, rendering: TextRendering, destination: TextDestination) {
        if self.shaper.set_text_rendering(rendering, destination) {
            self.invalidate_paint_configuration();
        }
    }

    pub fn text_rendering(&self) -> TextRendering {
        self.shaper.tracked_configuration().rendering
    }

    pub(crate) fn paint_text<T: PaintableText + ?Sized>(
        &mut self,
        canvas: &skia_safe::Canvas,
        text: &T,
        rect: lavashell_core::geometry::Rect,
        style: &TextStyle<Dp>,
    ) -> Result<(), DrawFailure> {
        let rendering = self.shaper.renderings_showing_weight(style);
        if self.paint_rasterized(canvas, text, rect, style, &rendering)? == Drawn::Painted {
            return Ok(());
        }
        match rendering {
            WeightRendering::OneFace => self.paint_on_canvas(canvas, text, rect, style),
            WeightRendering::TwoFaces(faces) => {
                self.paint_in_two_faces(canvas, text, rect, style, &faces)
            }
        }
    }

    fn paint_in_two_faces<T: PaintableText + ?Sized>(
        &mut self,
        canvas: &skia_safe::Canvas,
        text: &T,
        rect: lavashell_core::geometry::Rect,
        style: &TextStyle<Dp>,
        faces: &[FaceShare; 2],
    ) -> Result<(), DrawFailure> {
        let Some(scale) = device_scale(canvas) else {
            return Ok(());
        };
        let layers = self.choose_face_layers(canvas, style)?;
        let face_styles = faces
            .each_ref()
            .map(|face| layers.style_for_face_drawing(face, style));
        let requests = face_styles
            .each_ref()
            .map(|style| self.canvas_request(rect, style));
        let adding = layers.paint_for_second_face_layer(&self.text_blends, faces)?;
        let combining = layers.paint_for_first_face_layer();
        let tiles = self.resources.tiles.get_mut();
        let mut paragraphs = text.face_paragraphs(&mut self.shaper, requests)?;
        let mut ink = ink_on_canvas(rect, paragraphs.paragraph(Face::Lighter)?.paragraph)?;
        ink.join(ink_on_canvas(
            rect,
            paragraphs.paragraph(Face::Heavier)?.paragraph,
        )?);
        let bounds = outset_by_antialiasing(ink, scale);
        let _first_face_layer_restored_on_return =
            layers.save(&self.text_scopes, canvas, &bounds, &combining);
        draw_on_canvas(canvas, tiles, rect, paragraphs.paragraph(Face::Lighter)?)?;
        let _second_face_layer_restored_on_return =
            layers.save(&self.text_scopes, canvas, &bounds, &adding);
        draw_on_canvas(canvas, tiles, rect, paragraphs.paragraph(Face::Heavier)?)
    }

    fn choose_face_layers(
        &self,
        canvas: &skia_safe::Canvas,
        style: &TextStyle<Dp>,
    ) -> Result<FaceLayers, DrawFailure> {
        let configuration = self.shaper.tracked_configuration();
        let opaque_scope = self.text_scopes.opaque();
        if !(configuration.rendering).skia_draws_lcd(configuration.destination, opaque_scope) {
            return Ok(FaceLayers::ColorInLayer);
        }
        let range = ChannelRange::of(canvas.image_info().color_type());
        FaceLayers::lcd_coverage(&self.text_blends, style.color, range)
    }

    fn paint_rasterized<T: PaintableText + ?Sized>(
        &mut self,
        canvas: &skia_safe::Canvas,
        text: &T,
        rect: lavashell_core::geometry::Rect,
        style: &TextStyle<Dp>,
        rendering: &WeightRendering,
    ) -> Result<Drawn, DrawFailure> {
        let configuration = self.shaper.tracked_configuration();
        let input = raster::RasterInput {
            canvas,
            policy: configuration.rendering,
            destination: configuration.destination,
            opaque_scope: self.text_scopes.opaque(),
            rgb_blend: self.text_blends.rgb.clone(),
            style,
            foreground: crate::paint::color4f(style.color)?,
        };
        let Some(rasterization) = self.text_rasterizers.rasterization(&input) else {
            return Ok(Drawn::NotApplicable);
        };
        let coverage_in = |painter: &mut Self, source_style: &TextStyle<Dp>| {
            painter.rasterized_coverage(text, rect, &input, &rasterization, source_style)
        };
        let coverage = match rendering {
            WeightRendering::OneFace => Ok(coverage_in(self, &rasterization.style)?),
            WeightRendering::TwoFaces([lighter, heavier]) => {
                let in_face = |face: &FaceShare| TextStyle {
                    weight: face.weight,
                    ..rasterization.style.clone()
                };
                let lighter_share = scalar(lighter.share_of_coverage)?;
                let heavier_share = scalar(heavier.share_of_coverage)?;
                let lighter = coverage_in(self, &in_face(lighter))?;
                let heavier = coverage_in(self, &in_face(heavier))?;
                (lighter.zip(heavier))
                    .map(|(lighter, heavier)| {
                        Coverage::added_in_shares([
                            (lighter, lighter_share),
                            (heavier, heavier_share),
                        ])
                    })
                    .transpose()
            }
        };
        let drawn = coverage.and_then(|coverage| {
            (coverage.map(|coverage| rasterization.draw(&input, &coverage))).transpose()
        });
        Ok(declined_on_failure(&self.failures, drawn)
            .map_or(Drawn::NotApplicable, |()| Drawn::Painted))
    }

    fn rasterized_coverage<T: PaintableText + ?Sized>(
        &mut self,
        text: &T,
        rect: lavashell_core::geometry::Rect,
        input: &raster::RasterInput<'_>,
        rasterization: &raster::Rasterization,
        source_style: &TextStyle<Dp>,
    ) -> Result<Option<Coverage>, DrawFailure> {
        let source_request = ParagraphRequest {
            drawer: ParagraphDrawer::Rasterizer,
            rendering: rasterization.rendering,
            style: source_style,
            width: rect.width,
        };
        let tiles = self.resources.tiles.get_mut();
        text.with_paragraph(&mut self.shaper, &source_request, |source| {
            let origin = centered(rect, &source.paragraph.paragraph)?;
            let coverage =
                rasterization.coverage(input, origin, source.paragraph, source.key, tiles);
            Ok(declined_on_failure(&self.failures, coverage))
        })
    }

    fn paint_on_canvas<T: PaintableText + ?Sized>(
        &mut self,
        canvas: &skia_safe::Canvas,
        text: &T,
        rect: lavashell_core::geometry::Rect,
        style: &TextStyle<Dp>,
    ) -> Result<(), DrawFailure> {
        let request = self.canvas_request(rect, style);
        let tiles = self.resources.tiles.get_mut();
        text.with_paragraph(&mut self.shaper, &request, |target| {
            draw_on_canvas(canvas, tiles, rect, target)
        })
    }

    fn canvas_request<'s>(
        &self,
        rect: lavashell_core::geometry::Rect,
        style: &'s TextStyle<Dp>,
    ) -> ParagraphRequest<'s> {
        ParagraphRequest {
            drawer: ParagraphDrawer::Canvas,
            rendering: self.shaper.tracked_configuration().rendering,
            style,
            width: rect.width,
        }
    }
}

fn draw_on_canvas(
    canvas: &skia_safe::Canvas,
    tiles: &mut RasterCache,
    rect: lavashell_core::geometry::Rect,
    PaintableParagraph { paragraph, key }: PaintableParagraph<'_>,
) -> Result<(), DrawFailure> {
    let origin = centered(rect, &paragraph.paragraph)?;
    tiles.paragraph(canvas, paragraph, key, origin);
    Ok(())
}

fn ink_on_canvas(
    rect: lavashell_core::geometry::Rect,
    paragraph: &mut ShapedParagraph,
) -> Result<skia_safe::Rect, DrawFailure> {
    let origin = centered(rect, &paragraph.paragraph)?;
    Ok(paragraph.ink().with_offset(origin))
}

fn declined_on_failure<T>(
    failures: &Failures,
    outcome: Result<Option<T>, crate::Error>,
) -> Option<T> {
    outcome.unwrap_or_else(|error| {
        failures.record_caused(DrawFailure::SubpixelText, &error);
        None
    })
}

fn centered(
    rect: lavashell_core::geometry::Rect,
    paragraph: &Paragraph,
) -> Result<skia_safe::Point, DrawFailure> {
    let centering_offset = ((rect.height - dp_of(paragraph.height())) / 2.0).max(Dp::ZERO);
    let [x, y] = scalars([rect.x.get(), (rect.y + centering_offset).get()])?;
    Ok(skia_safe::Point::new(x, y))
}

#[cfg(test)]
mod canvas_tests;
#[cfg(test)]
mod retention_tests;
#[cfg(test)]
pub(crate) mod sample_fixtures;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) mod weight_blend_fixtures;
#[cfg(test)]
mod weight_blend_tests;
