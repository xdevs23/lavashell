// SPDX-License-Identifier: GPL-3.0-only
pub(crate) mod analysis;
#[cfg(test)]
mod analysis_tests;
mod brush;
mod configuration;
#[cfg(test)]
mod destination_tests;
mod geometry;
mod image;
pub(crate) mod opacity;
#[cfg(test)]
mod opacity_tests;
mod paragraph;
mod private;
mod scene_tree;
#[cfg(test)]
mod scene_tree_tests;
mod skia_geometry;
#[cfg(test)]
mod skia_geometry_tests;
mod stream;

use crate::destination::Destination;
use crate::failure::{DrawFailure, FailedDraw};
use crate::plan::Leaf;
use crate::scalar::scalar;
use brush::brush_paint;
pub(crate) use brush::{color4f, solid};
use lavashell_core::prelude::*;
pub(crate) use skia_geometry::{
    clip_device, device_scale, dp_of, max_image_side, rect_of, rounded, rrect, sklength, skmatrix,
    skpath, skpath_through,
};
pub use skia_geometry::{skpoint, skrect};
use skia_safe::textlayout::{FontCollection, Paragraph};
use skia_safe::{Canvas, PaintStyle};
use std::collections::HashMap;
pub use stream::{
    BufferAlpha, BufferFlag, HardwareBufferDescription, StreamFrames, StreamPixels,
    StreamRejection, UnsampledFormat,
};

pub struct Painter {
    pub(crate) blend_space: crate::BlendSpace,
    pub(crate) effect_registry: crate::effect_registry::EffectRegistry,
    pub(crate) effect_planner: crate::effect_registry::EffectPlanner,
    pub(crate) packet_cache: crate::packet_cache::PacketCache,
    pub(crate) analyses: analysis::Analyses,
    pub(crate) gpu_timing: crate::gpu_timing::GpuTiming,
    pub(crate) frame: u64,
    opacity: opacity::Opacity,
    pub(crate) failures: crate::failure::Failures,
    pub(crate) fonts: FontCollection,
    source_fonts: paragraph::Sources,
    pub(crate) registry: crate::Fonts,
    private_text: private::Private,
    pub(crate) paragraphs: crate::text_cache::TextCache,
    pub(crate) text_configuration: State<crate::text::Configuration>,
    pub(crate) paint_configuration: configuration::Configuration,
    pub(crate) text_rasterizers: crate::text::Rasterizers,
    pub(crate) text_scopes: crate::text::Scopes,
    pub(crate) images: HashMap<u64, (std::sync::Weak<[u8]>, skia_safe::Image)>,
    pub(crate) resources: crate::resources::PaintResources,
}

impl Painter {
    /// A painter whose field programs are sized for a device that reads at most `range` bytes of one uniform buffer.
    ///
    /// # Errors
    /// Fails when the painter cannot compile its shader programs or load its fonts.
    pub fn new(registry: &crate::Fonts, range: crate::UniformRange) -> Result<Self, crate::Error> {
        let room = crate::field::FieldRoom::new(range);
        let resources = crate::resources::PaintResources::new(&room)?;
        Ok(Self {
            blend_space: crate::BlendSpace::default(),
            effect_registry: crate::effect_registry::EffectRegistry::new(&room)?,
            effect_planner: crate::effect_registry::EffectPlanner::default(),
            packet_cache: crate::packet_cache::PacketCache::new(resources.held.clone()),
            analyses: analysis::Analyses::default(),
            gpu_timing: crate::gpu_timing::GpuTiming::default(),
            frame: 0,
            opacity: opacity::Opacity::new()?,
            failures: crate::failure::Failures::default(),
            fonts: registry.collection()?,
            source_fonts: paragraph::Sources::new(registry)?,
            registry: registry.clone(),
            private_text: private::Private::new(registry)?,
            paragraphs: crate::text_cache::TextCache::default(),
            text_configuration: State::new(crate::text::Configuration::default()),
            paint_configuration: configuration::Configuration::default(),
            text_rasterizers: crate::text::Rasterizers::new()?,
            text_scopes: crate::text::Scopes::default(),
            images: HashMap::new(),
            resources,
        })
    }

    pub(crate) fn with_raster_paths(mut self, enabled: bool) -> Self {
        self.resources.tiles = crate::raster::RasterCache::new(enabled).into();
        self
    }

    pub(crate) fn paragraph_input<'t>(
        &self,
        text: &'t str,
        style: &'t TextStyle<Dp>,
        width: Dp,
    ) -> Result<paragraph::ParagraphInput<'t>, DrawFailure> {
        paragraph::ParagraphInput::new(
            text,
            style,
            width,
            self.text_configuration.get().rendering,
            self.fonts.clone(),
        )
    }

    pub(crate) fn source_paragraph_input<'t>(
        &mut self,
        text: &'t str,
        style: &'t TextStyle<Dp>,
        width: Dp,
        rendering: crate::text::TextRendering,
    ) -> Result<paragraph::ParagraphInput<'t>, DrawFailure> {
        self.text_configuration.track();
        paragraph::ParagraphInput::new(
            text,
            style,
            width,
            rendering,
            self.source_fonts.collection(rendering),
        )
    }

    pub(crate) fn prepare_paragraph(
        &mut self,
        text: &str,
        style: &TextStyle<Dp>,
        width: Dp,
    ) -> Result<&mut Paragraph, DrawFailure> {
        let input = self.paragraph_input(text, style, width)?;
        Ok(&mut input
            .cached(&mut self.paragraphs, &self.registry)
            .shaped
            .paragraph)
    }

    #[cfg(test)]
    pub(crate) fn prepare_source_paragraph(
        &mut self,
        text: &str,
        style: &TextStyle<Dp>,
        width: Dp,
        rendering: crate::text::TextRendering,
    ) -> Result<&mut Paragraph, DrawFailure> {
        let input = self.source_paragraph_input(text, style, width, rendering)?;
        Ok(&mut input
            .cached(&mut self.paragraphs, &self.registry)
            .shaped
            .paragraph)
    }

    /// Paints the draw commands of `leaf`. Each effect is identified by the key of the leaf and by the position of
    /// the effect among the effects of the leaf.
    pub(crate) fn paint_commands(
        &mut self,
        canvas: &Canvas,
        leaf: &Leaf<'_>,
        streams: &StreamFrames,
    ) {
        let commands = leaf.packet.commands();
        self.images
            .retain(|_, (source, _)| source.strong_count() > 0);
        let saved = canvas.save();
        if leaf.analysis.layers && !leaf.analysis.has_effects() {
            // Layers inside the packet have the size of the clip, so they contain the same pixels in every pass.
            clip_device(canvas, leaf.device_extent);
        }
        let text_scope = self.text_scopes.enter(
            canvas,
            self.text_rasterizers
                .reads_destination(self.text_configuration.peek().rendering),
        );
        let mut destination = leaf.destination;
        let mut enclosing = Vec::new();
        let mut effects = 0;
        for (index, command) in commands.iter().enumerate() {
            self.text_scopes.observe(command);
            if self.effect_registry.identify(command).is_some() {
                self.render_effect(canvas, command, destination, leaf.key, effects);
                effects += 1;
                continue;
            }
            match command {
                DrawCommand::Restore => match enclosing.pop() {
                    Some(outer) => {
                        let _chained_canvas = canvas.restore();
                        destination = outer;
                    }
                    None => self.failures.record(DrawFailure::UnbalancedRestore),
                },
                DrawCommand::Save
                | DrawCommand::Opacity(_)
                | DrawCommand::OpacityLayer { .. }
                | DrawCommand::Transform(_)
                | DrawCommand::Clip { .. }
                | DrawCommand::EffectLayer { .. } => {
                    match self.open(canvas, leaf, index, command) {
                        Ok(Some(group)) => {
                            enclosing.push(destination);
                            destination = destination.enter(group);
                        }
                        Ok(None) => {}
                        Err(failure) => {
                            // The commands after the failed one would paint in another place or outside their clip.
                            self.failures.record(failure);
                            break;
                        }
                    }
                }
                drawing => {
                    if let Err(failure) = self.draw(canvas, drawing, streams) {
                        self.failures.record(failure);
                    }
                }
            }
        }
        let _chained_canvas = canvas.restore_to_count(saved);
        self.text_scopes.leave(text_scope);
    }

    /// Applies `command` to `canvas` for the commands after it. Returns the destination of the group opened by
    /// `command`, or None when `command` changes only the transform or the clip. A failure leaves the canvas as it
    /// was.
    fn open(
        &mut self,
        canvas: &Canvas,
        leaf: &Leaf<'_>,
        index: usize,
        command: &DrawCommand,
    ) -> Result<Option<Destination>, DrawFailure> {
        match command {
            DrawCommand::Save => {
                let _restored_by_restore_command = canvas.save();
                Ok(Some(Destination::Surface))
            }
            DrawCommand::Opacity(alpha) => {
                let reads_parent = leaf.analysis.reads_group(index);
                (self.opacity).save(canvas, scalar(*alpha)?, None, reads_parent, &self.failures);
                Ok(Some(Destination::faded(*alpha)))
            }
            DrawCommand::OpacityLayer { alpha, bounds } => {
                let bounds = (!leaf.analysis.expands_group(index))
                    .then(|| skrect(*bounds))
                    .transpose()?;
                let reads_parent = leaf.analysis.reads_group(index);
                (self.opacity).save(
                    canvas,
                    scalar(*alpha)?,
                    bounds,
                    reads_parent,
                    &self.failures,
                );
                Ok(Some(Destination::faded(*alpha)))
            }
            DrawCommand::Transform(affine) => {
                let _chained_canvas = canvas.concat(&skmatrix(affine)?);
                Ok(None)
            }
            DrawCommand::Clip { rect, corners } => {
                let _chained_canvas = canvas.clip_rrect(rrect(*rect, *corners)?, None, true);
                Ok(None)
            }
            DrawCommand::EffectLayer {
                bounds,
                blend,
                blur,
                filter,
            } => crate::effects::layer(
                canvas,
                *bounds,
                *blend,
                *blur,
                filter.as_ref(),
                leaf.analysis.expands_group(index),
            )
            .map(Some),
            _ => Ok(None),
        }
    }

    /// Draws one drawing command. The commands that open or close groups and layers draw nothing here.
    fn draw(
        &mut self,
        canvas: &Canvas,
        command: &DrawCommand,
        streams: &StreamFrames,
    ) -> Result<(), FailedDraw> {
        match command {
            DrawCommand::Shadow { shape, shadow } => {
                crate::effects::shadow(canvas, &self.resources, shape, *shadow)
            }
            DrawCommand::Stroke { shape, stroke } => {
                crate::effects::stroke(canvas, &self.resources, shape, *stroke)
            }
            DrawCommand::Fill { shape, brush } => {
                crate::effects::fill(canvas, &self.resources, shape, &brush_paint(brush)?)
            }
            DrawCommand::Image {
                image,
                destination,
                frame,
            } => (self.image(canvas, image, *destination, *frame)).map_err(FailedDraw::from),
            DrawCommand::StreamImage {
                image,
                destination,
                frame,
            } => (streams.draw(canvas, image, *destination, *frame)).map_err(FailedDraw::from),
            DrawCommand::Circle {
                center,
                radius,
                brush,
            } => {
                let paint = brush_paint(brush)?;
                let _chained_canvas =
                    canvas.draw_circle(skpoint(*center)?, sklength(*radius)?, &paint);
                Ok(())
            }
            DrawCommand::Path {
                start,
                segments,
                closed,
                brush,
                stroke,
            } => {
                let mut paint = brush_paint(brush)?;
                if let Some(width) = stroke {
                    let _chained_paint = paint
                        .set_style(PaintStyle::Stroke)
                        .set_stroke_width(sklength(*width)?)
                        .set_stroke_cap(skia_safe::PaintCap::Round)
                        .set_stroke_join(skia_safe::PaintJoin::Round);
                }
                let path = skpath_through(*start, segments, *closed)?;
                (self.resources.tiles.get_mut().draw(canvas, &path, &paint))
                    .map_err(FailedDraw::from)
            }
            DrawCommand::Text { text, rect, style } => {
                (self.paint_text(canvas, text, *rect, style)).map_err(FailedDraw::from)
            }
            DrawCommand::PrivateText { text, rect, style } => {
                (self.paint_private_text(canvas, text, *rect, style)).map_err(FailedDraw::from)
            }
            DrawCommand::Save
            | DrawCommand::Restore
            | DrawCommand::Clip { .. }
            | DrawCommand::Opacity(_)
            | DrawCommand::OpacityLayer { .. }
            | DrawCommand::Transform(_)
            | DrawCommand::EffectLayer { .. }
            | DrawCommand::Backdrop { .. } => Ok(()),
        }
    }
}

#[cfg(test)]
mod text_tests {
    use super::*;
    use crate::test_error::TestError;

    #[test]
    fn shaped_carets_use_graphemes_and_keep_both_sides_of_soft_wraps() -> Result<(), TestError> {
        let mut painter = Painter::new(&crate::Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
        let mut style = TextStyle::new("sans-serif", 20.sp(), Color::WHITE).resolve_unscaled();
        style.max_lines = None;
        let value = "e\u{301} 🙂 words that wrap onto several rows";
        let geometry = painter.geometry(value, &style, 110.dp());
        assert!(
            !geometry.carets.iter().any(|caret| caret.index == 1),
            "combining mark must share its cluster"
        );
        assert!(
            geometry
                .carets
                .iter()
                .all(|caret| value.is_char_boundary(caret.index))
        );
        assert_eq!(
            geometry.carets.last().map(|caret| caret.index),
            Some(value.len())
        );
        let wrap = geometry
            .carets
            .windows(2)
            .find(|pair| pair[0].index == pair[1].index && pair[0].rect.y < pair[1].rect.y)
            .ok_or(TestError::Missing("soft wrap"))?;
        assert_eq!(
            geometry
                .hit_caret(Point::new(wrap[0].rect.x, wrap[0].rect.y + 2.dp()))
                .rect
                .y,
            wrap[0].rect.y
        );
        assert_eq!(
            geometry
                .caret_on_line(wrap[1].index, Some(wrap[0].line))
                .rect
                .y,
            wrap[0].rect.y
        );
        Ok(())
    }
}
