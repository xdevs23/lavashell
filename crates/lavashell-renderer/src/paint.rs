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
#[cfg(any(test, feature = "test-support"))]
pub mod opacity_checks;
#[cfg(test)]
mod opacity_tests;
mod outline;
mod parent_copy;
#[cfg(any(test, feature = "test-support"))]
pub mod parent_copy_checks;
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
mod stream;

use crate::Error;
use crate::destination::{Destination, GroupOpacity};
use crate::failure::{DrawFailure, FailedDraw, Failures};
use crate::plan::{Area, Leaf};
use crate::resources::ImageCache;
use crate::scene_image::SceneImage;
use crate::text::Shaper;
use brush::brush_paint;
pub(crate) use brush::{color4f, solid};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::Presented;
use lavashell_core::surface::{PerSurface, SurfaceId};
use lavashell_core::unique_id::UniqueId;
pub use outline::{OutlineError, outline_path};
pub(crate) use skia_geometry::{
    clip_device, device_scale, dp_of, max_image_side, min_device_scale, rect_of, rounded, rrect,
    sklength, skmatrix, skpath, skpath_through,
};
pub use skia_geometry::{skpoint, skrect};
use skia_safe::{Canvas, IRect, PaintStyle, RoundOut};
use std::collections::HashMap;
pub use stream::{
    StreamFrames, StreamImportHandle, StreamPixels, StreamRejection, UnsampledFormat,
};

fn output_matrix(dp_to_output: &Affine) -> Result<skia_safe::Matrix, Error> {
    skmatrix(dp_to_output).map_err(Error::OutputTransform)
}

#[derive(Clone, Copy)]
pub struct SurfaceFrame<'a> {
    pub surface: SurfaceId,
    pub tree: Presented<'a>,
    pub dp_to_output: Affine,
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
    pub(crate) paint_configuration: configuration::Configuration,
    pub(crate) text_rasterizers: crate::text::Rasterizers,
    pub(crate) text_scopes: crate::text::Scopes,
    pub(crate) text_blends: crate::text::TextBlends,
    pub(crate) images: HashMap<UniqueId, (std::sync::Weak<[u8]>, skia_safe::Image)>,
    pub(crate) resources: crate::resources::PaintResources,
}

impl Painter {
    pub fn new(registry: &crate::Fonts, range: crate::UniformRange) -> Result<Self, crate::Error> {
        let room = crate::field::FieldRoom::new(range)?;
        let failures = Failures::default();
        let resources = crate::resources::PaintResources::new(&room, failures.clone())?;
        let loaded_fonts = registry.load()?;
        Ok(Self {
            blend_space: crate::BlendSpace::default(),
            effect_registry: crate::effect_registry::EffectRegistry::new(&room)?,
            effect_planner: crate::effect_registry::EffectPlanner::default(),
            packet_cache: crate::packet_cache::PacketCache::new(),
            analyses: analysis::Analyses::default(),
            gpu_timing: None,
            frame: 0,
            opacity: opacity::Opacity::new()?,
            shaper: Shaper::new(loaded_fonts, failures.clone()),
            failures,
            paint_configuration: configuration::Configuration::default(),
            text_rasterizers: crate::text::Rasterizers::new()?,
            text_scopes: crate::text::Scopes::default(),
            text_blends: crate::text::TextBlends::new()?,
            images: HashMap::new(),
            resources,
        })
    }

    pub fn without_device(registry: &crate::Fonts) -> Result<Self, crate::Error> {
        Self::new(registry, crate::UniformRange::WITHOUT_DEVICE)
    }

    #[must_use]
    pub fn with_raster_mode(mut self, mode: crate::raster::RasterMode) -> Self {
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
            dp_to_output,
            damage,
            clear,
            scene,
            shape,
            streams,
        } = frame;
        let scene = scene.map(|scene| scene.layer_of(canvas)).transpose()?;
        let target = IRect::from_size(canvas.base_layer_size());
        let dp_to_output = output_matrix(&dp_to_output)?;
        let damage = damage.and_then(|damage| {
            let pixels = match skrect(damage) {
                Ok(damage) => dp_to_output.map_rect(damage).0.round_out(),
                Err(failure) => {
                    self.failures.record(failure);
                    target
                }
            };
            IRect::intersect(&pixels, &target)
        });
        let damage = if self.paint_configuration.needs_reconstruction(surface) {
            Some(target)
        } else {
            damage
        };
        let Some(damage) = damage else {
            return Ok(());
        };
        self.paint_in_output_pixels(canvas, &dp_to_output, |painter| {
            let plan = painter.plan(canvas, surface, tree, shape);
            let whole =
                plan.unknown_sampling || painter.compositing_destination() == Destination::Layer;
            let repainted = plan.repaint(if whole { target } else { damage });
            clip_device(canvas, repainted);
            let _canvas_returned_for_chaining = canvas.clear(clear);
            if let Some(shape) = shape {
                let _canvas_returned_for_chaining = canvas.clip_path(shape, None, true);
            }
            if let Some(scene) = &scene {
                scene.draw();
            }
            painter.packet_cache.stats.damaged_pixels +=
                (repainted.width_64() * repainted.height_64()).cast_unsigned();
            painter.paint_tree(canvas, &plan, Area::Pixels(repainted), streams);
            painter.paint_configuration.painted(surface);
        });
        Ok(())
    }

    fn paint_in_output_pixels(
        &mut self,
        canvas: &Canvas,
        dp_to_output: &skia_safe::Matrix,
        paint: impl FnOnce(&mut Self),
    ) {
        let saved = canvas.save();
        let _canvas_returned_for_chaining = canvas.concat(dp_to_output);
        paint(self);
        let _canvas_returned_for_chaining = canvas.restore_to_count(saved);
    }

    pub fn shaper(&mut self) -> &mut Shaper {
        &mut self.shaper
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
                        let _canvas_returned_for_chaining = canvas.restore();
                        self.text_scopes.close();
                        destination = outer;
                    }
                    None => self.failures.record(DrawFailure::UnbalancedRestore),
                },
                group if group.is_structural() => match self.open(canvas, leaf, index, group) {
                    Ok(Some(group)) => {
                        self.text_scopes.open(group);
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
        let _canvas_returned_for_chaining = canvas.restore_to_count(saved);
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
            DrawCommand::Opacity(alpha) => self.open_opacity(canvas, leaf, index, *alpha, None),
            DrawCommand::OpacityLayer { alpha, bounds } => {
                self.open_opacity(canvas, leaf, index, *alpha, Some(*bounds))
            }
            DrawCommand::Transform(affine) => {
                let _canvas_returned_for_chaining = canvas.concat(&skmatrix(affine)?);
                Ok(None)
            }
            DrawCommand::Clip { rect, corners } => {
                let _canvas_returned_for_chaining =
                    canvas.clip_rrect(rrect(*rect, *corners)?, None, true);
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

    fn open_opacity(
        &self,
        canvas: &Canvas,
        leaf: &Leaf<'_>,
        index: usize,
        alpha: f64,
        bounds: Option<Rect>,
    ) -> Result<Option<Destination>, DrawFailure> {
        let opacity = GroupOpacity::new(alpha);
        let bounds = (bounds.filter(|_| !leaf.analysis.expands_group(index)))
            .map(skrect)
            .transpose()?;
        let reads_parent = leaf.analysis.reads_group(index);
        (self.opacity).save(canvas, opacity, bounds, reads_parent, &self.failures)?;
        Ok(Some(opacity.destination()))
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
                let _canvas_returned_for_chaining =
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
                    let _paint_returned_for_chaining = paint
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
                (self.paint_text(canvas, text.as_str(), *rect, style)).map_err(FailedDraw::from)
            }
            DrawCommand::PrivateText { text, rect, style } => {
                (self.paint_text(canvas, text, *rect, style)).map_err(FailedDraw::from)
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
        let mut shaper = Shaper::new(crate::Fonts::new().load()?, Failures::default());
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
