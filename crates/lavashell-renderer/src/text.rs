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
    /// Sets the text rendering policy, adjusted by [`TextRendering::for_destination`], and the destination of the
    /// text. Call it before measuring a frame. After a change, create the complete target again with the surface
    /// properties of the new destination, because subpixel text needs surface properties that match it. Ordinary
    /// transparent raster tiles have grayscale coverage on every device.
    pub fn set_text_rendering(&mut self, rendering: TextRendering, destination: TextDestination) {
        if self.shaper.set_text_rendering(rendering, destination) {
            self.invalidate_paint_configuration();
        }
    }

    /// The text rendering policy after its adjustment to the destination. A backend may still rasterize grayscale
    /// coverage into an intermediate transparent tile.
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
        // A source paragraph has the same line height as the paragraph in the text color. A rasterizer that applies
        // the text color while compositing therefore needs no paragraph in the text color, also not for centering.
        if let Some((provider, source_style, source_rendering)) =
            self.text_rasterizers.source(&input)
        {
            let source_input =
                self.source_paragraph_input(text, &source_style, rect.width, source_rendering)?;
            let source = self.shaper.cached(source_input);
            let drawn = self.text_rasterizers.draw(
                provider,
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

/// The origin at which `paragraph` is painted centered vertically in `rect`, in pixels of the canvas.
///
/// # Errors
/// Returns [`DrawFailure::NumberRange`] when a finite coordinate of the origin lies beyond the range of Skia
/// scalars.
pub(crate) fn centered(
    rect: lavashell_core::geometry::Rect,
    paragraph: &Paragraph,
) -> Result<(f32, f32), DrawFailure> {
    let centering_offset = ((rect.height - dp_of(paragraph.height())) / 2.0).max(Dp::ZERO);
    let [x, y] = scalars([rect.x.get(), (rect.y + centering_offset).get()])?;
    Ok((x, y))
}

#[cfg(test)]
mod canvas_tests;
#[cfg(test)]
mod retention_tests;
#[cfg(test)]
mod tests;
