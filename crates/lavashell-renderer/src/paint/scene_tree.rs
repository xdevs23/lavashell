// SPDX-License-Identifier: GPL-3.0-only
use super::{Painter, StreamFrames, rounded, skmatrix};
use crate::destination::Destination;
use crate::failure::DrawFailure;
use crate::plan::{Area, Plan, Planned, PlannedGroup};
use crate::scalar::scalar;
use crate::text::SavedScope;
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::Presented;
use lavashell_core::surface::SurfaceId;
use skia_safe::{Canvas, IRect};

impl Painter {
    pub fn paint_scene(
        &mut self,
        canvas: &Canvas,
        surface: SurfaceId,
        tree: Presented<'_>,
        density: Density,
    ) {
        let plan = self.plan(canvas, surface, tree, density, None);
        let streams = StreamFrames::default();
        self.paint_tree(canvas, &plan, Area::Everything, density, &streams);
    }

    pub(crate) fn paint_tree(
        &mut self,
        canvas: &Canvas,
        plan: &Plan<'_>,
        area: Area,
        density: Density,
        streams: &StreamFrames,
    ) {
        let scale = match scalar(density.get()) {
            Ok(scale) => scale,
            Err(failure) => {
                self.failures.record(failure);
                return;
            }
        };
        self.shaper.begin_paint(plan.surface);
        self.resources.begin_frame(plan.surface);
        if !plan.all_leaves_cacheable {
            self.forget_transient_effect_inputs(plan.surface);
        }
        let saved = canvas.save();
        let text_scope = (self.text_rasterizers)
            .reads_destination(self.shaper.configuration().rendering)
            .then(|| self.text_scopes.enter(canvas));
        let _canvas_returned_for_chaining = canvas.scale((scale, scale));
        self.begin_compositing(canvas);
        self.paint_steps(canvas, &plan.steps, area, plan.pass, streams);
        let _canvas_returned_for_chaining = canvas.restore_to_count(saved);
        if let Some(entered) = text_scope {
            self.text_scopes.leave(entered);
        }
    }

    fn paint_steps(
        &mut self,
        canvas: &Canvas,
        steps: &[Planned<'_>],
        area: Area,
        pass: u64,
        streams: &StreamFrames,
    ) {
        for step in steps {
            let (_restored_on_drop, inside) = match step {
                Planned::Leaf(leaf) => {
                    match area.repainted(leaf.device_extent) {
                        Some(extent) => self.paint_leaf(canvas, leaf, extent, streams, pass),
                        None => self.packet_cache.stats.skipped_leaves += 1,
                    }
                    continue;
                }
                Planned::Group { group, steps } => match self.open_group(canvas, group) {
                    Ok(saved) => (saved, steps),
                    Err(failure) => {
                        self.failures.record(failure);
                        continue;
                    }
                },
                Planned::Graft(steps) => {
                    (self.text_scopes.save(canvas, Destination::Surface), steps)
                }
            };
            self.paint_steps(canvas, inside, area, pass, streams);
        }
    }

    fn open_group<'c>(
        &mut self,
        canvas: &'c Canvas,
        group: &PlannedGroup,
    ) -> Result<SavedScope<'c>, DrawFailure> {
        let values = &group.values;
        let transform = skmatrix(&values.transform)?;
        let clip = values.clip.as_ref().map(rounded).transpose()?;
        let saved = self.text_scopes.save(canvas, group.opacity.destination());
        let _canvas_returned_for_chaining = canvas.concat(&transform);
        if let Some(clip) = clip {
            let _canvas_returned_for_chaining = canvas.clip_rrect(clip, None, true);
        }
        if group.opacity.destination() == Destination::Layer {
            super::clip_device(canvas, group.device_extent.unwrap_or(IRect::new_empty()));
        }
        (self.opacity).save(
            canvas,
            group.opacity,
            None,
            group.reads_parent,
            &self.failures,
        )?;
        Ok(saved)
    }
}
