// SPDX-License-Identifier: GPL-3.0-only
use super::{DrawTarget, EffectRegistry, EffectRequest, ShapePainter, SurfaceAccess};
use crate::failure::DrawFailure;
use crate::{
    Painter,
    destination::Destination,
    vulkan::{GpuSample, SampleKind, SampleTag},
};
use lavashell_core::prelude::Density;
use lavashell_core::scene::DrawCommand;
use lavashell_core::scene::tree::{NodeKey, Presented};
use lavashell_core::surface::SurfaceId;
use lavashell_effect_quality::{
    Choice, DrawId, DrawTimeKey, DrawTimes, EffectDraw, EffectId, FrameBudget, QualityChooser,
    VariantId, VariantOption,
};
use skia_safe::Canvas;
use std::collections::BTreeMap;
use timing::GpuSpan;

#[derive(Clone, Debug, PartialEq)]
pub struct EffectInput {
    pub scene_generation: u64,
    pub(crate) cache_allowed: bool,
}

/// Whether planning changed the quality variant chosen for an effect draw. After a change, the pixels stored from
/// earlier frames of the surface must be painted again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualityChange {
    Unchanged,
    Changed,
}

#[derive(Default)]
struct SurfaceState {
    choices: BTreeMap<DrawId, Choice>,
    frames_waited: BTreeMap<DrawId, u64>,
    planned: bool,
    /// Whether the effect draws of the planned frame are timed.
    timed: bool,
}

#[derive(Clone, Copy)]
struct PlannedFrame {
    budget: FrameBudget,
    scene_generation: u64,
    density: Density,
}

#[derive(Default)]
pub(crate) struct EffectPlanner {
    chooser: Option<Box<dyn QualityChooser>>,
    draw_times: DrawTimes,
    surfaces: BTreeMap<SurfaceId, SurfaceState>,
    inputs: BTreeMap<DrawId, EffectInput>,
    /// A blank surface of the same size and format as the target, on which each variant without a first measurement
    /// is drawn once and timed. All measurements of one frame use it.
    blank_copy: Option<skia_safe::Surface>,
    pub surface: Option<SurfaceId>,
}

impl EffectPlanner {
    /// Drops the blank Skia surface of the planner. Call it while the GPU context that created the surface is still
    /// alive.
    pub(crate) fn retire_gpu_resources(&mut self) {
        self.blank_copy = None;
    }

    pub(crate) fn current_surface(&self) -> SurfaceId {
        self.surface.unwrap_or(SurfaceId::UNSPECIFIED)
    }

    pub(super) fn record_draw_time(&mut self, sample: GpuSample) {
        let surface = sample.tag.surface;
        // A sample can arrive after its surface was retired.
        if !self.surfaces.contains_key(&surface) {
            return;
        }
        match sample.tag.kind {
            SampleKind::Effect(key) => self.draw_times.record(surface, key, sample.elapsed),
            SampleKind::FirstEffectDraw(key) => {
                (self.draw_times).record_first_measurement(surface, key, sample.elapsed);
            }
            SampleKind::Frame => {}
        }
    }
}

fn draw_target(canvas: &Canvas) -> DrawTarget {
    DrawTarget {
        pixels_per_dp: f64::from(canvas.local_to_device_as_3x3().max_scale()),
    }
}

struct Planning {
    frame: PlannedFrame,
    cacheable: bool,
    draws: Vec<EffectDraw>,
    inputs: Vec<(DrawId, EffectInput)>,
}

impl Planning {
    fn new(frame: PlannedFrame) -> Self {
        Self {
            frame,
            cacheable: true,
            draws: Vec::new(),
            inputs: Vec::new(),
        }
    }
}

impl Painter {
    pub(crate) fn forget_transient_effect_inputs(&mut self) {
        if let Some(surface) = self.effect_planner.surface {
            self.effect_registry.forget_surface(surface);
        }
        for input in self.effect_planner.inputs.values_mut() {
            input.cache_allowed = false;
        }
    }

    pub(crate) fn invalidate_effect_snapshots(&mut self) {
        for input in self.effect_planner.inputs.values_mut() {
            input.cache_allowed = false;
        }
        for surface in self.effect_planner.surfaces.keys() {
            self.effect_registry.forget_surface(*surface);
        }
    }

    pub fn effect_registry_mut(&mut self) -> &mut EffectRegistry {
        &mut self.effect_registry
    }

    pub fn effect_quality(&mut self, chooser: impl QualityChooser + 'static) {
        self.effect_planner.chooser = Some(Box::new(chooser));
    }

    pub fn uses_quality_chooser(&self) -> bool {
        self.effect_planner.chooser.is_some()
    }

    /// Drops everything stored for `surface`, such as draw times, caches and quality choices. Call it once the
    /// surface is gone. Its identifier must not be used again.
    pub fn forget_effect_surface(&mut self, surface: SurfaceId) {
        (self.effect_planner.surfaces).retain(|kept, _| *kept != surface);
        self.effect_planner.draw_times.forget_surface(surface);
        self.gpu_timing.forget(surface);
        self.paint_configuration.forget(surface);
        self.shaper.forget_surface(surface);
        self.resources.forget_surface(surface);
        self.analyses.forget_surface(surface);
        self.packet_cache.forget_surface(surface);
        self.effect_registry.forget_surface(surface);
        if let Some(chooser) = &mut self.effect_planner.chooser {
            chooser.forget_surface(surface);
        }
    }

    /// Collects the GPU times of completed draws, then chooses the quality variant of each effect draw for the next
    /// frame, which is painted at `density` within `budget`. `frame` is the sequence number of that frame and is
    /// attached to the GPU samples of its draws. `tree` contains everything shown on the surface named by `budget`,
    /// whose identifier is never reused for another surface. Paint this surface before planning another one. A GPU
    /// timer that cannot be read is recorded as a failure. The times of the other draws still count.
    pub fn plan_scene_effects(
        &mut self,
        tree: Presented<'_>,
        budget: FrameBudget,
        frame: u64,
        scene_generation: u64,
        density: Density,
    ) -> QualityChange {
        self.frame = frame;
        let collected = self.gpu_timing.collect();
        for failure in &collected.failures {
            self.failures
                .record_caused(DrawFailure::GpuTimestamp, failure);
        }
        for sample in collected.samples {
            self.effect_planner.record_draw_time(sample);
        }
        self.effect_planner.inputs.clear();
        self.effect_planner.blank_copy = None;
        self.effect_planner.surface = Some(budget.surface);
        if self.effect_planner.chooser.is_none() {
            (self.effect_planner.surfaces).retain(|kept, _| *kept != budget.surface);
            return QualityChange::Unchanged;
        }
        let planned = PlannedFrame {
            budget,
            scene_generation,
            density,
        };
        self.choose_effects(tree, Planning::new(planned))
    }

    fn choose_effects(&mut self, tree: Presented<'_>, mut planning: Planning) -> QualityChange {
        for leaf in tree.paint_order().leaves() {
            let pixels_per_dp = planning.frame.density.get() * leaf.world.stretch();
            self.collect_effects(
                leaf.packet.commands(),
                leaf.key,
                pixels_per_dp,
                &mut planning,
            );
        }
        let (budget, draws) = (planning.frame.budget, planning.draws);
        self.effect_planner.inputs = planning.inputs.into_iter().collect();
        let Some(chooser) = self.effect_planner.chooser.as_mut() else {
            return QualityChange::Unchanged;
        };
        let plan = chooser.choose(budget, &draws);
        let state = (self.effect_planner.surfaces)
            .entry(budget.surface)
            .or_default();
        let choices = plan.choices();
        for (id, choice) in &choices {
            let frames_waited = state.frames_waited.entry(*id).or_default();
            *frames_waited = if *choice == Choice::Wait {
                frames_waited.saturating_add(1)
            } else {
                0
            };
        }
        (state.frames_waited).retain(|id, _| choices.contains_key(id));
        // Timing a draw flushes the GPU work around it, and its samples include that cost. The draws are timed only
        // while the settled plan has no time yet or its time is under the GPU time budget.
        state.timed = (plan.settled_time())
            .is_none_or(|time| budget.gpu_time.is_none_or(|gpu_time| time < gpu_time));
        (self.effect_planner.draw_times).retain_chosen(budget.surface, &plan);
        let changed = state.planned && state.choices != choices;
        state.choices = choices;
        state.planned = true;
        if !changed {
            return QualityChange::Unchanged;
        }
        self.clear_packet_cache();
        QualityChange::Changed
    }

    fn collect_effects(
        &mut self,
        commands: &[DrawCommand],
        leaf: NodeKey,
        pixels_per_dp: f64,
        planning: &mut Planning,
    ) {
        let surface = planning.frame.budget.surface;
        let target = DrawTarget { pixels_per_dp };
        let mut ordinal = 0;
        for command in commands {
            if let Some(effect) = self.effect_registry.identify(command) {
                let id = DrawId {
                    surface,
                    leaf,
                    ordinal,
                    effect,
                };
                ordinal += 1;
                planning.inputs.push((
                    id,
                    EffectInput {
                        scene_generation: planning.frame.scene_generation,
                        cache_allowed: planning.cacheable && self.uses_quality_chooser(),
                    },
                ));
                let draw_times = &self.effect_planner.draw_times;
                let options = (self
                    .effect_registry
                    .variants(effect, command, target)
                    .into_iter())
                .map(|variant| VariantOption {
                    variant,
                    gpu_time: draw_times.gpu_time(surface, DrawTimeKey { draw: id, variant }),
                })
                .collect();
                planning.draws.push(EffectDraw {
                    id,
                    frames_waited: (self.effect_planner.surfaces.get(&surface))
                        .and_then(|state| state.frames_waited.get(&id))
                        .copied()
                        .unwrap_or(0),
                    options,
                });
            }
            planning.cacheable &= !command.is_transient();
        }
    }

    pub(crate) fn render_effect(
        &mut self,
        canvas: &Canvas,
        command: &DrawCommand,
        destination: Destination,
        leaf: NodeKey,
        ordinal: u64,
    ) {
        let Some(effect) = self.effect_registry.identify(command) else {
            return;
        };
        let surface = self.effect_planner.current_surface();
        let id = DrawId {
            surface,
            leaf,
            ordinal,
            effect,
        };
        let supported = self
            .effect_registry
            .variants(effect, command, draw_target(canvas));
        let state = self.effect_planner.surfaces.get(&surface);
        // A planned variant that this canvas cannot draw falls back to the best one it can.
        let variant = match state.and_then(|state| state.choices.get(&id)).copied() {
            Some(Choice::Wait) => return,
            Some(Choice::Draw(variant)) if supported.contains(&variant) => variant,
            _ => match supported.first() {
                Some(best) => *best,
                None => return,
            },
        };
        let timed = state.is_some_and(|state| state.timed) && self.uses_quality_chooser();
        if timed {
            self.measure_first_draws(canvas, effect, id, command, &supported);
        }
        let input = self.effect_planner.inputs.get(&id).cloned();
        let request = EffectRequest {
            surface,
            id,
            variant,
            command,
            input: input.as_ref(),
            access: SurfaceAccess::new(destination, input.as_ref()),
            painter: ShapePainter::new(&self.resources, &self.failures),
        };
        let registry = &mut self.effect_registry;
        let mut render = || registry.render(effect, canvas, &request);
        let profiler = self.gpu_timing.profiler().filter(|_| timed);
        let span = profiler.and_then(|profiler| GpuSpan::new(canvas, profiler));
        let tag = SampleTag {
            kind: SampleKind::Effect(DrawTimeKey { draw: id, variant }),
            surface,
            frame: self.frame,
        };
        match span {
            Some(mut span) => {
                if let Err(cause) = span.time(tag, render) {
                    self.failures
                        .record_caused(DrawFailure::GpuTimestamp, &cause);
                }
            }
            None => render(),
        }
    }

    /// Draws each variant of draw `id` that has no first measurement yet once on a blank copy of `canvas`, and
    /// times it. All measurements of one frame use the same blank copy. A variant whose timing fails is marked as
    /// unmeasurable and gets its times only from the samples of its regular draws.
    fn measure_first_draws(
        &mut self,
        canvas: &Canvas,
        effect: EffectId,
        id: DrawId,
        command: &DrawCommand,
        variants: &[VariantId],
    ) {
        let surface = self.effect_planner.current_surface();
        let draw_times = &mut self.effect_planner.draw_times;
        let unmeasured = (variants.iter())
            .map(|variant| DrawTimeKey {
                draw: id,
                variant: *variant,
            })
            .filter(|key| draw_times.needs_first_measurement(surface, *key))
            .collect::<Vec<_>>();
        if unmeasured.is_empty() {
            return;
        }
        let Some(profiler) = self.gpu_timing.profiler() else {
            return;
        };
        let Some(mut span) = GpuSpan::new(canvas, profiler) else {
            return;
        };
        let Some(copy) = blank_copy(&mut self.effect_planner.blank_copy, canvas) else {
            return;
        };
        let blank = copy.canvas();
        let saved = blank.save();
        if let Some(clip) = canvas.device_clip_bounds() {
            let _chained_canvas = blank.clip_irect(clip, None);
        }
        let _chained_canvas = blank.set_matrix(&canvas.local_to_device());
        for key in unmeasured {
            let _chained_canvas = blank.clear(skia_safe::Color::TRANSPARENT);
            let request = EffectRequest {
                surface,
                id,
                variant: key.variant,
                command,
                input: None,
                access: SurfaceAccess::Current,
                painter: ShapePainter::new(&self.resources, &self.failures),
            };
            let tag = SampleTag {
                kind: SampleKind::FirstEffectDraw(key),
                surface,
                frame: self.frame,
            };
            let registry = &mut self.effect_registry;
            match span.time(tag, || registry.render(effect, blank, &request)) {
                Ok(()) => draw_times.await_first_measurement(surface, key),
                Err(cause) => {
                    self.failures
                        .record_caused(DrawFailure::GpuTimestamp, &cause);
                    draw_times.mark_unmeasurable(surface, key);
                }
            }
        }
        let _chained_canvas = blank.restore_to_count(saved);
    }
}

/// The surface in `slot` when it has the size and format of the surface of `canvas`. Otherwise a new surface with
/// that size and format, stored in `slot`, or `None` when Skia cannot create it.
fn blank_copy<'a>(
    slot: &'a mut Option<skia_safe::Surface>,
    canvas: &Canvas,
) -> Option<&'a mut skia_safe::Surface> {
    let info = canvas.image_info();
    if slot.as_ref().is_none_or(|copy| copy.image_info() != info) {
        *slot = canvas.new_surface(&info, None);
    }
    slot.as_mut()
}

#[cfg(test)]
mod draw_time_tests;
#[cfg(test)]
mod storage_tests;
mod timing;
