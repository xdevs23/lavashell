// SPDX-License-Identifier: GPL-3.0-only
use super::{DrawTarget, EffectRegistry, EffectRequest, ShapePainter, SurfaceAccess};
use crate::failure::DrawFailure;
use crate::gpu_timing::GpuTiming;
use crate::{
    Painter,
    destination::Destination,
    vulkan::{GpuSample, SampleKind, SampleTag},
};
use lavashell_core::prelude::Density;
use lavashell_core::scene::DrawCommand;
use lavashell_core::scene::tree::{NodeKey, Presented};
use lavashell_core::surface::{PerSurface, SurfaceId};
use lavashell_effect_quality::{
    Choice, DrawId, DrawTimeKey, DrawTimes, EffectDraw, EffectId, FrameBudget, OfferedVariant,
    QualityChooser, VariantId, VariantOption,
};
use skia_safe::Canvas;
use std::collections::BTreeMap;
use timing::GpuSpan;

#[derive(Clone, Debug, PartialEq)]
pub struct EffectInput {
    pub scene_generation: u64,
    pub(crate) cache_allowed: bool,
}

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
    blank_copy: Option<skia_safe::Surface>,
}

impl EffectPlanner {
    pub(crate) fn retire_gpu_resources(&mut self) {
        self.blank_copy = None;
    }

    pub(super) fn record_draw_time(&mut self, sample: GpuSample) {
        let surface = sample.tag.surface;
        if !self.surfaces.contains_key(&surface) {
            return;
        }
        match sample.tag.kind {
            SampleKind::Effect(key) => self.draw_times.record(key, sample.elapsed),
            SampleKind::FirstEffectDraw(key) => {
                (self.draw_times).record_first_measurement(key, sample.elapsed);
            }
            SampleKind::Frame => {}
        }
    }
}

impl PerSurface for EffectPlanner {
    fn forget_surface(&mut self, surface: SurfaceId) {
        (self.surfaces).retain(|kept, _| *kept != surface);
        self.draw_times.forget_surface(surface);
        if let Some(chooser) = &mut self.chooser {
            chooser.forget_surface(surface);
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
    pub(crate) fn forget_transient_effect_inputs(&mut self, surface: SurfaceId) {
        self.effect_registry.forget_surface(surface);
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

    pub fn plan_scene_effects(
        &mut self,
        tree: Presented<'_>,
        budget: FrameBudget,
        frame: u64,
        scene_generation: u64,
        density: Density,
    ) -> QualityChange {
        self.frame = frame;
        let collected = (self.gpu_timing.as_mut())
            .map(GpuTiming::collect)
            .unwrap_or_default();
        for failure in &collected.failures {
            self.failures
                .record_caused(DrawFailure::GpuTimestamp, failure);
        }
        for sample in collected.samples {
            self.effect_planner.record_draw_time(sample);
        }
        self.effect_planner.inputs.clear();
        self.effect_planner.blank_copy = None;
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
                .map(|offered| VariantOption {
                    offered,
                    gpu_time: draw_times.gpu_time(DrawTimeKey {
                        draw: id,
                        variant: offered.variant,
                    }),
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
        surface: SurfaceId,
        leaf: NodeKey,
        ordinal: u64,
    ) {
        let Some(effect) = self.effect_registry.identify(command) else {
            return;
        };
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
        let variant = match state.and_then(|state| state.choices.get(&id)).copied() {
            Some(Choice::Wait) => return,
            Some(Choice::Draw(variant)) if is_offered(&supported, variant) => variant,
            _ => match OfferedVariant::most_preferred(&supported) {
                Some(best) => best.variant,
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
        let profiler = (self.gpu_timing.as_mut())
            .filter(|_| timed)
            .map(GpuTiming::profiler);
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

    fn measure_first_draws(
        &mut self,
        canvas: &Canvas,
        effect: EffectId,
        id: DrawId,
        command: &DrawCommand,
        variants: &[OfferedVariant],
    ) {
        let surface = id.surface;
        let draw_times = &mut self.effect_planner.draw_times;
        let unmeasured = (variants.iter())
            .map(|offered| DrawTimeKey {
                draw: id,
                variant: offered.variant,
            })
            .filter(|key| draw_times.needs_first_measurement(*key))
            .collect::<Vec<_>>();
        if unmeasured.is_empty() {
            return;
        }
        let Some(timing) = self.gpu_timing.as_mut() else {
            return;
        };
        let Some(mut span) = GpuSpan::new(canvas, timing.profiler()) else {
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
                Ok(()) => draw_times.await_first_measurement(key),
                Err(cause) => {
                    self.failures
                        .record_caused(DrawFailure::GpuTimestamp, &cause);
                    draw_times.mark_unmeasurable(key);
                }
            }
        }
        let _chained_canvas = blank.restore_to_count(saved);
    }
}

fn is_offered(offered: &[OfferedVariant], variant: VariantId) -> bool {
    offered.iter().any(|offered| offered.variant == variant)
}

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
