// SPDX-License-Identifier: GPL-3.0-only
pub(crate) mod analysis;
#[cfg(test)]
mod analysis_tests;
mod brush;
mod configuration;
#[cfg(test)]
mod destination_tests;
mod image;
pub(crate) mod opacity;
#[cfg(test)]
mod opacity_tests;
mod outline;
mod parent_copy;
#[cfg(test)]
mod parent_copy_tests;
mod private;
mod scene_tree;
#[cfg(test)]
mod scene_tree_effect_tests;
#[cfg(test)]
mod scene_tree_tests;
mod skia_geometry;
#[cfg(test)]
mod skia_geometry_tests;
mod sources;
mod stream;

use crate::Error;
use crate::destination::Destination;
use crate::failure::{DrawFailure, FailedDraw, Failures};
use crate::plan::{Area, Leaf};
use crate::resources::ImageCache;
use crate::scalar::{scalar, scalars};
use crate::text::{ParagraphInput, Shaper};
use crate::vulkan::SceneImage;
use brush::brush_paint;
pub(crate) use brush::{color4f, solid};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::Presented;
use lavashell_core::surface::{PerSurface, SurfaceId};
use lavashell_core::unique_id::UniqueId;
pub use outline::{OutlineError, clip_path, outline_path};
pub(crate) use skia_geometry::{
    clip_device, device_scale, dp_of, max_image_side, rect_of, rounded, rrect, sklength, skmatrix,
    skpath, skpath_through,
};
pub use skia_geometry::{skpoint, skrect};
use skia_safe::{Canvas, IRect, PaintStyle, RoundOut};
use std::collections::HashMap;
pub use stream::{StreamFrames, StreamPixels, StreamRejection, UnsampledFormat};

#[derive(Clone, Copy)]
pub struct SurfaceFrame<'a> {
    pub surface: SurfaceId,
    pub tree: Presented<'a>,
    pub density: Density,
    pub damage: Option<Rect>,
    pub clear: skia_safe::Color,
    pub scene: Option<&'a SceneImage>,
    pub shape: Option<&'a skia_safe::Path>,
    pub streams: &'a StreamFrames,
}

pub struct Painter {
    pub(crate) blend_space: crate::BlendSpace,
    pub(crate) effect_registry: crate::effect_registry::EffectRegistry,
    pub(crate) effect_planner: crate::effect_registry::EffectPlanner,
    pub(crate) packet_cache: crate::packet_cache::PacketCache,
    pub(crate) analyses: analysis::Analyses,
    pub(crate) gpu_timing: Option<crate::gpu_timing::GpuTiming>,
    pub(crate) frame: u64,
    opacity: opacity::Opacity,
    pub(crate) failures: Failures,
    pub(crate) shaper: Shaper,
    source_fonts: sources::Sources,
    pub(crate) paint_configuration: configuration::Configuration,
    pub(crate) text_rasterizers: crate::text::Rasterizers,
    pub(crate) text_scopes: crate::text::Scopes,
    pub(crate) images: HashMap<UniqueId, (std::sync::Weak<[u8]>, skia_safe::Image)>,
    pub(crate) resources: crate::resources::PaintResources,
}

impl Painter {
    pub fn new(registry: &crate::Fonts, range: crate::UniformRange) -> Result<Self, crate::Error> {
        let room = crate::field::FieldRoom::new(range)?;
        let failures = Failures::default();
        let resources = crate::resources::PaintResources::new(&room, failures.clone())?;
        Ok(Self {
            blend_space: crate::BlendSpace::default(),
            effect_registry: crate::effect_registry::EffectRegistry::new(&room)?,
            effect_planner: crate::effect_registry::EffectPlanner::default(),
            packet_cache: crate::packet_cache::PacketCache::new(),
            analyses: analysis::Analyses::default(),
            gpu_timing: None,
            frame: 0,
            opacity: opacity::Opacity::new()?,
            shaper: Shaper::new(registry, failures.clone())?,
            failures,
            source_fonts: sources::Sources::new(registry)?,
            paint_configuration: configuration::Configuration::default(),
            text_rasterizers: crate::text::Rasterizers::new()?,
            text_scopes: crate::text::Scopes::default(),
            images: HashMap::new(),
            resources,
        })
    }

    pub(crate) fn with_raster_mode(mut self, mode: crate::raster::RasterMode) -> Self {
        self.resources.tiles = crate::raster::RasterCache::new(mode, self.failures.clone()).into();
        self
    }

    pub fn retire_gpu_resources(&mut self) {
        self.paint_configuration.clear();
        self.clear_packet_cache();
        self.images.clear();
        self.resources.retire();
        self.effect_planner.retire_gpu_resources();
        self.replace_effect_registry(crate::effect_registry::EffectRegistry::empty());
        self.resources.held.set(ImageCache::Backdrops, 0);
    }

    pub(crate) fn replace_effect_registry(
        &mut self,
        registry: crate::effect_registry::EffectRegistry,
    ) {
        self.effect_registry = registry;
        self.analyses.clear();
    }

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        let per_surface: [&mut dyn PerSurface; 7] = [
            &mut self.effect_planner,
            &mut self.gpu_timing,
            &mut self.paint_configuration,
            &mut self.shaper,
            &mut self.resources,
            &mut self.analyses,
            &mut self.effect_registry,
        ];
        for state in per_surface {
            state.forget_surface(surface);
        }
        (self.packet_cache).forget_surface(surface, &self.resources.held);
    }

    pub fn paint_surface(&mut self, canvas: &Canvas, frame: SurfaceFrame<'_>) -> Result<(), Error> {
        let SurfaceFrame {
            surface,
            tree,
            density,
            damage,
            clear,
            scene,
            shape,
            streams,
        } = frame;
        let scene = scene.map(|scene| scene.layer_of(canvas)).transpose()?;
        let target = IRect::from_size(canvas.base_layer_size());
        let pixels = |length: Dp| density.to_px(length).get();
        let damage = damage.map(|damage| {
            let lengths = [damage.x, damage.y, damage.width, damage.height].map(pixels);
            match scalars(lengths) {
                Ok([x, y, width, height]) => {
                    skia_safe::Rect::from_xywh(x, y, width, height).round_out()
                }
                Err(failure) => {
                    self.failures.record(failure);
                    target
                }
            }
        });
        let damage = if self.paint_configuration.needs_reconstruction(surface) {
            Some(target)
        } else {
            damage
        };
        let Some(damage) = damage else {
            return Ok(());
        };
        let plan = self.plan(canvas, surface, tree, density, shape);
        let whole = plan.unknown_sampling || self.compositing_destination() == Destination::Layer;
        let repainted = plan.repaint(if whole { target } else { damage });
        let saved = canvas.save();
        let _chained_canvas = canvas.clip_irect(repainted, None).clear(clear);
        if let Some(shape) = shape {
            let _chained_canvas = canvas.clip_path(shape, None, true);
        }
        if let Some(scene) = &scene {
            scene.draw();
        }
        self.packet_cache.stats.damaged_pixels +=
            (repainted.width_64() * repainted.height_64()).cast_unsigned();
        self.paint_tree(canvas, &plan, Area::Pixels(repainted), density, streams);
        self.paint_configuration.painted(surface);
        let _chained_canvas = canvas.restore_to_count(saved);
        Ok(())
    }

    pub fn shaper(&mut self) -> &mut Shaper {
        &mut self.shaper
    }

    pub(crate) fn source_paragraph_input<'t>(
        &mut self,
        text: &'t str,
        style: &'t TextStyle<Dp>,
        width: Dp,
        rendering: crate::text::TextRendering,
    ) -> Result<ParagraphInput<'t>, DrawFailure> {
        self.shaper.observe_rendering();
        ParagraphInput::new(
            text,
            style,
            width,
            rendering,
            self.source_fonts.collection(rendering),
        )
    }

    #[cfg(test)]
    pub(crate) fn prepare_source_paragraph(
        &mut self,
        text: &str,
        style: &TextStyle<Dp>,
        width: Dp,
        rendering: crate::text::TextRendering,
    ) -> Result<&mut skia_safe::textlayout::Paragraph, DrawFailure> {
        let input = self.source_paragraph_input(text, style, width, rendering)?;
        Ok(&mut self.shaper.cached(input).shaped.paragraph)
    }

    pub(crate) fn paint_commands(
        &mut self,
        canvas: &Canvas,
        leaf: &Leaf<'_>,
        extent: IRect,
        streams: &StreamFrames,
    ) {
        let commands = leaf.packet.commands();
        self.images
            .retain(|_, (source, _)| source.strong_count() > 0);
        let saved = canvas.save();
        if leaf.analysis.layers && !leaf.analysis.has_effects() {
            clip_device(canvas, extent);
        }
        let text_scope = (self.text_rasterizers)
            .reads_destination(self.shaper.configuration().rendering)
            .then(|| self.text_scopes.enter(canvas));
        let mut destination = leaf.destination;
        let mut enclosing = Vec::new();
        let mut effects = 0;
        for (index, command) in commands.iter().enumerate() {
            self.text_scopes.observe(command);
            if self.effect_registry.identify(command).is_some() {
                self.render_effect(
                    canvas,
                    command,
                    destination,
                    leaf.surface,
                    leaf.key,
                    effects,
                );
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
                group if group.is_structural() => match self.open(canvas, leaf, index, group) {
                    Ok(Some(group)) => {
                        enclosing.push(destination);
                        destination = destination.inside(group);
                    }
                    Ok(None) => {}
                    Err(failure) => {
                        self.failures.record(failure);
                        break;
                    }
                },
                drawing => {
                    if let Err(failure) = self.draw(canvas, drawing, streams) {
                        self.failures.record(failure);
                    }
                }
            }
        }
        let _chained_canvas = canvas.restore_to_count(saved);
        if let Some(entered) = text_scope {
            self.text_scopes.leave(entered);
        }
    }

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
                Ok(Some(Destination::for_opacity(*alpha)))
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
                Ok(Some(Destination::for_opacity(*alpha)))
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
            } if leaf.analysis.reads_group(index)
                && parent_copy::restores_effect_layer(*blend, *blur, filter.as_ref()) =>
            {
                let bounds = (!leaf.analysis.expands_group(index)).then_some(*bounds);
                parent_copy::save_effect_layer(canvas, leaf.analysis.layer_area(index), bounds)?;
                Ok(Some(Destination::Layer))
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
        let mut shaper = Shaper::new(&crate::Fonts::new(), Failures::default())?;
        let mut style = TextStyle::new("sans-serif", 20.sp(), Color::WHITE).resolve_unscaled();
        style.max_lines = None;
        let value = "e\u{301} 🙂 words that wrap onto several rows";
        let geometry = shaper.geometry(value, &style, 110.dp());
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
