// SPDX-License-Identifier: GPL-3.0-only
use super::{Painter, StreamFrames, rounded, skmatrix};
use crate::destination::Destination;
use crate::failure::DrawFailure;
use crate::plan::{Area, Plan, Planned, PlannedGroup};
use crate::scalar::scalar;
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::Presented;
use skia_safe::Canvas;

impl Painter {
    pub fn paint_scene(&mut self, canvas: &Canvas, tree: Presented<'_>, density: Density) {
        self.paint_scene_with_streams(canvas, tree, density, &StreamFrames::default());
    }

    pub fn paint_scene_with_streams(
        &mut self,
        canvas: &Canvas,
        tree: Presented<'_>,
        density: Density,
        streams: &StreamFrames,
    ) {
        let plan = self.plan(canvas, tree, density, None);
        self.paint_tree(canvas, &plan, Area::Everything, density, streams);
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
        let surface = self.effect_planner.current_surface();
        self.shaper.begin_paint(surface);
        self.resources.begin_frame(surface);
        if !plan.all_leaves_cacheable {
            self.forget_transient_effect_inputs();
        }
        let saved = canvas.save();
        let text_scope = self.text_scopes.enter(
            canvas,
            self.text_rasterizers
                .reads_destination(self.shaper.configuration().rendering),
        );
        let _chained_canvas = canvas.scale((scale, scale));
        self.begin_compositing(canvas);
        self.paint_steps(canvas, &plan.steps, area, plan.pass, streams);
        let _chained_canvas = canvas.restore_to_count(saved);
        self.text_scopes.leave(text_scope);
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
            let (saved, inside) = match step {
                Planned::Leaf(leaf) if area.repaints(leaf.device_extent) => {
                    self.paint_leaf(canvas, leaf, streams, pass);
                    continue;
                }
                Planned::Leaf(_) => {
                    self.packet_cache.stats.skipped_leaves += 1;
                    continue;
                }
                Planned::Group { group, steps } => {
                    let saved = canvas.save();
                    if let Err(failure) = self.open_group(canvas, group) {
                        self.failures.record(failure);
                        let _chained_canvas = canvas.restore_to_count(saved);
                        continue;
                    }
                    (saved, steps)
                }
                Planned::Graft(steps) => {
                    let saved = canvas.save();
                    self.text_scopes.open(true);
                    (saved, steps)
                }
            };
            self.paint_steps(canvas, inside, area, pass, streams);
            let _chained_canvas = canvas.restore_to_count(saved);
            self.text_scopes.close();
        }
    }

    /// Applies the transform, the clip and the opacity of `group` to `canvas`. A failure leaves the canvas and the
    /// text scopes as they were.
    fn open_group(&mut self, canvas: &Canvas, group: &PlannedGroup) -> Result<(), DrawFailure> {
        let values = &group.values;
        let transform = skmatrix(&values.transform)?;
        let clip = values.clip.as_ref().map(rounded).transpose()?;
        let alpha = scalar(values.opacity)?;
        let _chained_canvas = canvas.concat(&transform);
        if let Some(clip) = clip {
            let _chained_canvas = canvas.clip_rrect(clip, None, true);
        }
        self.text_scopes
            .open(group.destination == Destination::Surface);
        if group.destination == Destination::Layer {
            // The layer has the size of the clip, so it contains the same pixels in every pass.
            super::clip_device(canvas, group.device_extent);
        }
        (self.opacity).save(canvas, alpha, None, group.reads_parent, &self.failures);
        Ok(())
    }
}
