// SPDX-License-Identifier: GPL-3.0-only
use super::open_surfaces::OpenSurfaces;
use super::{ViewActor, send_request};
use crate::cpu_time::CpuStamp;
use crate::panels::{PanelContent, PanelRequests};
use crate::renderer_actor::{Rendered, RendererInput};
use crate::scene_pipeline::{FrameStatus, SceneSink};
use crate::scheduler::{FrameKind, FrameSequence, SchedulerInput};
use crate::shape::{SurfaceShape, WindowShape};
use crate::shape_geometry::ShapeGeometry;
use crate::view_actions::ActionBatch;
use crate::view_actor::{Stopped, ViewError};
use lavashell_actor::Closed;
use lavashell_core::frame::{FrameReport, FrameStage, FrameTime, Submission};
use lavashell_core::geometry::{ScaledViewport, Size, Viewport};
use lavashell_core::output::OutputInfo;
use lavashell_core::surface::{SurfaceChange, SurfaceId};
use lavashell_core::view::{Failures, FrameError, View};
use lavashell_platform::{
    Drawn, FrameOutcome, PanelRequest, Platform, PlatformRequest, SurfaceContents,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(super) struct FrameStart {
    wall: Instant,
    view_cpu: Option<Duration>,
    timing: FrameTime,
    scaled_viewport: ScaledViewport,
}

impl FrameStart {
    fn composed(self) -> ComposedFrame {
        ComposedFrame {
            timing: self.timing,
            viewport: self.scaled_viewport.viewport,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ComposedFrame {
    pub(super) timing: FrameTime,
    pub(super) viewport: Viewport,
}

pub(super) struct StateCommit {
    pub(super) composed: Option<ComposedFrame>,
    pub(super) panel: Option<PanelRequests>,
}

#[derive(Clone, Copy)]
enum FrameProgress {
    Idle,
    Composed(FrameStart),
    Drawing {
        start: FrameStart,
        panel: Option<PanelRequests>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewChange {
    Changed,
    Unchanged,
}

#[derive(Clone, Copy)]
struct LastFrame {
    scaled_viewport: ScaledViewport,
    contents: SurfaceContents,
}

pub(super) struct ViewSurface {
    pub(super) composition_order: i32,
    frame: FrameProgress,
    last_frame: Option<LastFrame>,
    committing: Option<StateCommit>,
    output: Option<OutputInfo>,
    shape: SurfaceShape,
}

impl ViewSurface {
    pub(super) fn new(shape: Option<WindowShape>, composition_order: i32) -> Self {
        Self {
            composition_order,
            frame: FrameProgress::Idle,
            last_frame: None,
            committing: None,
            output: None,
            shape: SurfaceShape::new(shape),
        }
    }

    pub(super) fn needs_visual_frame(&self, view: &View) -> bool {
        view.needs_visual_frame() || self.shape.needs_frame()
    }

    pub(super) fn contents(&self, scaled_viewport: ScaledViewport) -> SurfaceContents {
        match self.last_frame {
            Some(last) if last.scaled_viewport == scaled_viewport => last.contents,
            _ => SurfaceContents::Outdated,
        }
    }

    pub(super) fn panel_content(&self, size: Size) -> PanelContent {
        PanelContent {
            size,
            input: self.shape.accepts_input(),
        }
    }

    pub(super) fn start_commit(&mut self, panel: Option<PanelRequests>) {
        let composed = match self.frame {
            FrameProgress::Composed(start) => {
                self.frame = FrameProgress::Idle;
                Some(start.composed())
            }
            FrameProgress::Idle | FrameProgress::Drawing { .. } => None,
        };
        self.committing = Some(StateCommit { composed, panel });
    }

    pub(super) fn finish_commit(&mut self) -> Option<StateCommit> {
        self.committing.take()
    }

    pub(super) fn finish_drawing(
        &mut self,
        scaled_viewport: ScaledViewport,
        contents: SurfaceContents,
        time: FrameTime,
    ) -> Option<(FrameStart, Option<PanelRequests>)> {
        self.last_frame = Some(LastFrame {
            scaled_viewport,
            contents,
        });
        let FrameProgress::Drawing { start, panel } = self.frame else {
            return None;
        };
        if start.timing.sequence != time.sequence {
            return None;
        }
        self.frame = FrameProgress::Idle;
        Some((start, panel))
    }
}

impl<P: Platform> ViewActor<P> {
    pub(super) fn render(&mut self) -> Result<(), ViewError> {
        self.execute_surface_requests()?;
        self.commit_built_states()?;
        self.submit_transfers()?;
        self.accessibility_updates()?;
        let mut actions = self.action_queue.take_batch();
        let order = self.surfaces.composition_order();
        self.advance_scene()?;
        let mut started = self.pacing.take_started_frames();
        let mut composed = false;
        for &id in &order {
            let Some((kind, timing)) = started.remove(&id) else {
                continue;
            };
            if !self.surfaces.records().is_open(id) {
                continue;
            }
            match kind {
                FrameKind::Compose => {
                    if !composed {
                        self.notify_frame(timing);
                        composed = true;
                    }
                    self.compose_surface(id, timing, &mut actions)?;
                }
                FrameKind::Render => self.render_surface(id, timing)?,
            }
        }
        self.action_queue.requeue(actions);
        for &id in &order {
            let queued = self.action_queue.has_pending_for(id);
            self.want_frame(id, queued);
        }
        Ok(())
    }

    fn want_frame(&self, id: SurfaceId, actions_queued: bool) {
        if let Some(wants) = self.surfaces.wanted_frame(id, actions_queued) {
            self.schedule(wants);
        }
    }

    fn advance_scene(&self) -> Result<(), ViewError> {
        if self.pacing.scene_due(self.clock.now())
            && let Some(scaled_viewport) = self.surfaces.first_configured_scaled_viewport()
            && let Some(renderer) = self.actors.renderer()
        {
            renderer.send(RendererInput::AdvanceScene(scaled_viewport))?;
        }
        Ok(())
    }

    fn notify_frame(&mut self, timing: FrameTime) {
        let since_last_tick = self.pacing.tick(self.clock.now());
        if let Some(tick) = self.observers.tick.as_mut() {
            tick(since_last_tick);
        }
        if let Some(callback) = self.observers.frame.as_mut() {
            callback(timing);
        }
    }

    fn compose_surface(
        &mut self,
        id: SurfaceId,
        timing: FrameTime,
        actions: &mut ActionBatch,
    ) -> Result<(), ViewError> {
        let wall = Instant::now();
        let cpu = self.observers.cpu_timing().stamp();
        let change = self.compose_view(id, timing, actions)?;
        self.update_panel_after_compose(id, change)?;
        let shape_changes = self.update_shape(id)?;
        let sequence = Some(FrameSequence(timing.sequence));
        let sink = self.actors.scene.sink(id, sequence);
        let status = send_surface_state(sink, &self.surfaces, id, shape_changes)?;
        let (surface, scaled_viewport) = self.surfaces.open_surface(id)?;
        surface.frame = FrameProgress::Composed(FrameStart {
            wall,
            view_cpu: cpu.and_then(CpuStamp::elapsed),
            timing,
            scaled_viewport,
        });
        self.publish_accessibility(id)?;
        if status == FrameStatus::Open {
            self.schedule(SchedulerInput::Built {
                surface: id,
                sequence,
            });
        }
        Ok(())
    }

    fn compose_view(
        &mut self,
        id: SurfaceId,
        timing: FrameTime,
        actions: &mut ActionBatch,
    ) -> Result<ViewChange, ViewError> {
        let (view, viewport) = self.surfaces.open_view(id)?;
        let mut failed = |failures| self.observers.report_failures(id, failures);
        let mut frame = |view: &mut View, sink: &mut SceneSink<'_>| {
            view.frame_into_at(viewport, &mut self.shaper, timing, sink)
        };
        let mut change = if view.needs_visual_frame() {
            ViewChange::Changed
        } else {
            ViewChange::Unchanged
        };
        send_frame(
            view,
            self.actors.scene.sink(id, None),
            &mut failed,
            &mut frame,
        )?;
        actions.run(id, view);
        if view.needs_visual_frame() {
            change = ViewChange::Changed;
            send_frame(
                view,
                self.actors.scene.sink(id, None),
                &mut failed,
                &mut frame,
            )?;
        }
        Ok(change)
    }

    fn update_panel_after_compose(
        &mut self,
        id: SurfaceId,
        change: ViewChange,
    ) -> Result<(), ViewError> {
        if change == ViewChange::Changed {
            self.surfaces.measure_panel(id, &mut self.shaper)?;
        }
        if let Some(keyboard) = self.surfaces.update_panel(id) {
            let request = PanelRequest::SetKeyboard(id, keyboard);
            send_request(&self.requests, PlatformRequest::Panel(request))?;
        }
        Ok(())
    }

    fn update_shape(&mut self, id: SurfaceId) -> Result<Vec<SurfaceChange>, ViewError> {
        let (surface, scaled_viewport) = self.surfaces.open_surface(id)?;
        let Viewport { size, density } = scaled_viewport.viewport;
        let mut clip = None;
        let shape_changes = surface.shape.update(size, density, |outline| {
            let geometry = ShapeGeometry::new(outline, size)?;
            clip = Some(geometry.clip);
            Ok::<_, ViewError>(geometry.input_cells)
        })?;
        if let Some(path) = clip
            && let Some(renderer) = self.actors.renderer()
        {
            let outline = surface.shape.clips_paint().then_some(path);
            renderer.send(RendererInput::Outline {
                surface: id,
                outline,
            })?;
        }
        Ok(shape_changes)
    }

    fn render_surface(&mut self, id: SurfaceId, timing: FrameTime) -> Result<(), ViewError> {
        let Some(renderer) = self.actors.renderer() else {
            self.schedule(SchedulerInput::Unrendered {
                surface: id,
                frame: timing,
            });
            return Ok(());
        };
        let built = self.actors.scene.built(id);
        let panel = self.surfaces.panel_requests(id, &built.state);
        let (surface, scaled_viewport) = self.surfaces.open_surface(id)?;
        let start = match surface.frame {
            FrameProgress::Composed(composed) => composed,
            FrameProgress::Idle | FrameProgress::Drawing { .. } => FrameStart {
                wall: Instant::now(),
                view_cpu: Some(Duration::ZERO),
                timing,
                scaled_viewport,
            },
        };
        surface.frame = FrameProgress::Drawing { start, panel };
        renderer.send(RendererInput::Render {
            surface: id,
            time: start.timing,
            scaled_viewport: start.scaled_viewport,
            built,
        })?;
        Ok(())
    }

    pub(super) fn rendered(&mut self, rendered: &Rendered) {
        let Rendered {
            surface: id,
            time,
            scaled_viewport,
            drawn,
            contents,
            next_change,
            cost,
            cpu_time,
        } = *rendered;
        let outcome = drawn.outcome;
        let finished = (self.surfaces).rendered(id, scaled_viewport, contents, time, outcome);
        if let Some(start) = finished {
            self.report_frame(id, drawn, start, cpu_time);
        }
        self.schedule(SchedulerInput::Published {
            surface: id,
            next_change,
        });
        self.schedule(SchedulerInput::Rendered {
            surface: id,
            frame: time,
            outcome,
            cost,
        });
    }

    fn report_frame(
        &mut self,
        id: SurfaceId,
        drawn: Drawn,
        start: FrameStart,
        render_cpu: Option<Duration>,
    ) {
        let Some((surface, view, viewport)) = self.surfaces.open_surface_and_view(id) else {
            return;
        };
        self.observers
            .observe_view(id, start.timing, viewport, view);
        if let Some(info) = drawn.output
            && surface.output != Some(info)
        {
            surface.output = Some(info);
            if let Some(observer) = &mut self.observers.output {
                observer(id, info);
            }
        }
        if drawn.outcome != FrameOutcome::Shown {
            view.invalidate();
            return;
        }
        view.record_presentation(start.timing.presentation);
        if let Some(observer) = &mut self.observers.frame_report
            && let Some(view_cpu) = start.view_cpu
            && let Some(render_cpu) = render_cpu
        {
            observer(FrameReport {
                surface: id,
                timing: start.timing,
                stage: FrameStage::Submitted,
                observed: self.clock.now(),
                presentation_time: None,
                metrics: None,
                flags: None,
                submission: Some(Submission {
                    render_cost: start.wall.elapsed(),
                    cpu_time: view_cpu + render_cpu,
                    view: view.stats(),
                    render: drawn.stats,
                    scene: drawn.scene_generation,
                    gpu: drawn.gpu,
                }),
            });
        }
    }
}

pub(super) fn send_frame(
    view: &mut View,
    mut sink: SceneSink<'_>,
    failed: impl FnOnce(Failures),
    frame: impl FnOnce(&mut View, &mut SceneSink<'_>) -> Result<Failures, FrameError<Closed>>,
) -> Result<(), ViewError> {
    let shows_scene = sink.any_frame_ended();
    match frame(view, &mut sink) {
        Ok(failures) => {
            failed(failures);
            Ok(())
        }
        Err(FrameError::Sink(Closed)) => Err(ViewError::Stopped(Stopped::Scene)),
        Err(error) if shows_scene => {
            warn_whole_scene_next(&error);
            Ok(())
        }
        Err(error) => Err(ViewError::ShowScene(error)),
    }
}

fn warn_whole_scene_next(error: impl std::error::Error) {
    log::warn!(
        "a frame failed, and the whole scene is sent with the next frame: {}",
        lavashell_error_chain::message_chain(error)
    );
}

pub(super) fn send_surface_state(
    mut sink: SceneSink<'_>,
    surfaces: &OpenSurfaces,
    id: SurfaceId,
    changes: impl IntoIterator<Item = SurfaceChange>,
) -> Result<FrameStatus, ViewError> {
    let stopped = |Closed| ViewError::Stopped(Stopped::Scene);
    for change in changes
        .into_iter()
        .chain(surfaces.panel_geometry_change(id))
    {
        sink.set_state(change).map_err(stopped)?;
    }
    sink.end_state_frame().map_err(stopped)
}

#[cfg(test)]
mod tests;
