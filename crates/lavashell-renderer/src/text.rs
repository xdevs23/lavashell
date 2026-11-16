// SPDX-License-Identifier: GPL-3.0-only
mod approximate;
mod coverage;
mod glyphs;
mod lcd;
mod policy;
mod raster;
mod scope;
mod shaper;

use crate::failure::DrawFailure;
use crate::paint::dp_of;
use crate::scalar::scalars;
pub(crate) use glyphs::Glyphs;
use lavashell_core::units::Dp;
pub use policy::{Hinting, PixelGeometry, TextAntialiasing, TextDestination, TextRendering};
pub(crate) use raster::{Drawn, RasterInput, Rasterizers};
pub(crate) use scope::Scopes;
pub(crate) use shaper::ParagraphInput;
pub use shaper::Shaper;
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

    pub(crate) fn paint_text(
        &mut self,
        canvas: &skia_safe::Canvas,
        text: &str,
        rect: lavashell_core::geometry::Rect,
        style: &lavashell_core::scene::TextStyle<Dp>,
    ) -> Result<(), DrawFailure> {
        let configuration = self.shaper.configuration();
        let input = raster::RasterInput {
            canvas,
            policy: configuration.rendering,
            destination: configuration.destination,
            opaque_scope: self.text_scopes.opaque(),
            style,
            foreground: crate::paint::color4f(style.color)?,
        };
        if let Some(rasterization) = self.text_rasterizers.rasterization(&input) {
            let source_input = self.source_paragraph_input(
                text,
                &rasterization.style,
                rect.width,
                rasterization.rendering,
            )?;
            let source = self.shaper.cached(source_input);
            let drawn = rasterization.draw(
                &input,
                centered(rect, &source.shaped.paragraph)?,
                &mut source.shaped,
                source.key,
                self.resources.tiles.get_mut(),
            );
            match drawn {
                Ok(Drawn::Painted) => return Ok(()),
                Ok(Drawn::NotApplicable) => {}
                Err(error) => self
                    .failures
                    .record_caused(DrawFailure::SubpixelText, &error),
            }
        }
        let input = self.shaper.paragraph_input(text, style, rect.width)?;
        let cached = self.shaper.cached(input);
        let origin = centered(rect, &cached.shaped.paragraph)?;
        (self.resources.tiles.get_mut()).paragraph(canvas, &mut cached.shaped, cached.key, origin);
        Ok(())
    }
}

pub(crate) fn centered(
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
mod tests;
