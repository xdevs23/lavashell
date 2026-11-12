// SPDX-License-Identifier: GPL-3.0-only
use super::scene_frames::{send_frame, send_surface_state};
use super::{ViewActor, send_request};
use crate::cpu_time::CpuStamp;
use crate::panels::{PanelContent, PanelRequests};
use crate::renderer_actor::{Rendered, RendererInput};
use crate::scene_pipeline::{FrameStatus, SceneSink};
use crate::scheduler::{FrameKind, FrameSequence, SchedulerInput, ViewState};
use crate::shape::{SurfaceShape, WindowShape};
use crate::shape_geometry::ShapeGeometry;
use crate::surface_records::SurfaceRecords;
use crate::view_actions::ActionBatch;
use crate::view_actor::ViewError;
use lavashell_core::frame::{FrameReport, FrameStage, FrameTime, Submission};
use lavashell_core::geometry::{PixelScale, Viewport};
use lavashell_core::output::OutputInfo;
use lavashell_core::surface::{SurfaceChange, SurfaceId};
use lavashell_core::view::View;
use lavashell_platform::{PanelRequest, Platform, PlatformRequest};
use lavashell_renderer::{Drawn, FrameOutcome, SurfaceContents};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// When a frame started on the wall clock, the CPU time that the views thread spent on the frame, and the time, the
/// viewport and the scale of the frame.
#[derive(Clone, Copy)]
struct FrameStart {
    wall: Instant,
    /// Measured only while an observer watches the frames.
    view_cpu: Option<Duration>,
    timing: FrameTime,
    viewport: Viewport,
    scale: PixelScale,
}

/// How far the frame of a surface got between its compose and the report of its draw.
#[derive(Clone, Copy)]
enum FrameProgress {
    Idle,
    /// The view composed the frame. The frame is drawn in the viewport and at the scale of the compose, and observers
    /// measure it from the start of the compose.
    Composed(FrameStart),
    /// The renderer draws the frame. A shown frame applies `panel`.
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

/// The viewport and the scale of the frame drawn last for a surface, and whether the target of the surface contains
/// that frame.
#[derive(Clone, Copy)]
struct LastFrame {
    viewport: Viewport,
    scale: PixelScale,
    contents: SurfaceContents,
}

/// The frames, the shape and the place in the composition order of an open surface.
pub(super) struct ViewSurface {
    composition_order: i32,
    frame: FrameProgress,
    last_frame: Option<LastFrame>,
    /// The panel requests applied by the commit without a frame that the renderer has not reported yet.
    pub(super) committing: Option<PanelRequests>,
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

    /// Whether the target of the surface contains the frame drawn last in `viewport` at `scale`.
    pub(super) fn contents(&self, viewport: Viewport, scale: PixelScale) -> SurfaceContents {
        match self.last_frame {
            Some(last) if last.viewport == viewport && last.scale == scale => last.contents,
            _ => SurfaceContents::Outdated,
        }
    }
}

impl<P: Platform> ViewActor<P> {
    /// Makes the renderer commit each surface state that waits for a commit without a frame, then runs the frame
    /// started by the frame scheduler for each surface, in composition order. A compose frame sends the frames of the
    /// view and the surface state to the scene pipeline, and a render frame makes the renderer draw the newest scene
    /// of the surface. After the frames, every surface reports what its view needs, with the deadline after
    /// composing, and the scheduler starts the next frame from that report.
    pub(super) fn render(&mut self) -> Result<(), ViewError> {
        self.execute_surface_requests()?;
        self.commit_built_states()?;
        self.submit_transfers()?;
        self.accessibility_updates()?;
        let mut actions = self.action_queue.take_batch();
        let order = self.composition_order();
        self.advance_scene()?;
        // The started frames of a surface that is not open, or no longer known, are dropped.
        let mut started = self.pacing.take_started_frames();
        let mut composed = false;
        for &id in &order {
            let Some((kind, timing)) = started.remove(&id) else {
                continue;
            };
            if !self.records.is_open(id) {
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
        // View actions queued during composition run in another frame, started at once by a surface that is ready.
        for &id in &order {
            let queued = self.action_queue.has_pending_for(id);
            self.want_frame(id, queued);
        }
        Ok(())
    }

    /// Reports to the frame scheduler what the view of surface `id` needs, while the surface is open. The view of a
    /// configured surface changed when it needs a visual frame or `actions_queued` is true.
    fn want_frame(&self, id: SurfaceId, actions_queued: bool) {
        let (Some(surface), Some(view)) = (self.surfaces.get(&id), self.records.open_view(id))
        else {
            return;
        };
        let state = if !self.records.is_configured(id) {
            ViewState::Unconfigured
        } else if surface.needs_visual_frame(view) || actions_queued {
            ViewState::Changed
        } else {
            ViewState::Unchanged
        };
        self.schedule(SchedulerInput::Wants {
            surface: id,
            view: state,
            deadline: view.next_deadline(),
        });
    }

    /// Makes the renderer advance the scene source for the first configured surface while the source is due, so the
    /// scene work runs on the GPU while the views are composed.
    fn advance_scene(&self) -> Result<(), ViewError> {
        if self.pacing.scene_due(self.clock.now())
            && let Some(&surface) =
                (self.surfaces.keys()).find(|&&id| self.records.is_configured(id))
            && let Some(viewport) = self.records.open_viewport(surface)
            && let Some(scale) = self.records.open_scale(surface)
        {
            let advance = RendererInput::AdvanceScene { viewport, scale };
            self.actors.renderer.send(advance)?;
        }
        Ok(())
    }

    /// Calls the tick callback of the app, and the frame callback of the app with `timing`, the timing of the first
    /// frame composed in a render.
    fn notify_frame(&mut self, timing: FrameTime) {
        let since_last_tick = self.pacing.tick(self.clock.now());
        if let Some(tick) = self.observers.tick.as_mut() {
            tick(since_last_tick);
        }
        if let Some(callback) = self.observers.frame.as_mut() {
            callback(timing);
        }
    }

    fn composition_order(&self) -> Vec<SurfaceId> {
        let mut order: Vec<_> = (self.surfaces.iter())
            .map(|(&id, surface)| (surface.composition_order, id))
            .collect();
        order.sort_unstable();
        order.into_iter().map(|(_, id)| id).collect()
    }

    /// Reads the CPU clock of the thread only while an observer watches the frames.
    fn cpu_stamp(&self) -> Option<CpuStamp> {
        self.observers
            .frame_report
            .as_ref()
            .and_then(|_| CpuStamp::now())
    }

    /// Composes the view of surface `id` for `timing` with its queued actions, and sends the frames of the view to the
    /// scene pipeline, followed by the state of the surface for the frame. The frame renders once the scene builder
    /// built the state, so the state and the frames of the view render together. When the state waits for the whole
    /// scene after a failed frame, the frame renders at once.
    fn compose_surface(
        &mut self,
        id: SurfaceId,
        timing: FrameTime,
        actions: &mut ActionBatch,
    ) -> Result<(), ViewError> {
        let wall = Instant::now();
        let cpu = self.cpu_stamp();
        let change = self.compose_view(id, timing, actions)?;
        self.update_panel_after_compose(id, change)?;
        let shape_changes = self.update_shape(id)?;
        let sequence = Some(FrameSequence(timing.sequence));
        let sink = self.actors.scene.sink(id, sequence);
        let status = send_surface_state(sink, &self.panels, id, shape_changes)?;
        let scale = (self.records.open_scale(id)).ok_or(ViewError::SurfaceNotOpen(id))?;
        let (surface, _, viewport) = open_parts(&mut self.surfaces, &mut self.records, id)?;
        surface.frame = FrameProgress::Composed(FrameStart {
            wall,
            view_cpu: cpu.and_then(CpuStamp::elapsed),
            timing,
            viewport,
            scale,
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

    /// Sends the frames of the view of surface `id` for `timing` to the scene pipeline, before and after the queued
    /// actions of the view run.
    fn compose_view(
        &mut self,
        id: SurfaceId,
        timing: FrameTime,
        actions: &mut ActionBatch,
    ) -> Result<ViewChange, ViewError> {
        let (_, view, viewport) = open_parts(&mut self.surfaces, &mut self.records, id)?;
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

    /// Measures the panel of surface `id` again when the view changed, and sends the keyboard mode of the panel to the
    /// platform when it changed. The composed view can change the size of a panel that fits its content, or the
    /// panel layout can change its margins, and the geometry for that is sent with the frame of the compose, so that
    /// the host applies it with the buffer of that frame.
    fn update_panel_after_compose(
        &mut self,
        id: SurfaceId,
        change: ViewChange,
    ) -> Result<(), ViewError> {
        if change == ViewChange::Changed {
            let view = (self.records.open_view_mut(id)).ok_or(ViewError::SurfaceNotOpen(id))?;
            self.panels.measure(id, view, &mut self.shaper)?;
        }
        if let Some(keyboard) = self.panels.update(id) {
            let request = PanelRequest::SetKeyboard(id, keyboard);
            send_request(&self.requests, PlatformRequest::Panel(request))?;
        }
        Ok(())
    }

    /// Fits the shape of surface `id` to the viewport of the surface, sends the outline that clips the surface to the
    /// renderer when the shape changed, and returns the surface changes of the shape.
    fn update_shape(&mut self, id: SurfaceId) -> Result<Vec<SurfaceChange>, ViewError> {
        let (surface, _, viewport) = open_parts(&mut self.surfaces, &mut self.records, id)?;
        let Viewport { size, density } = viewport;
        let mut clip = None;
        let shape_changes = surface.shape.update(size, density, |outline| {
            let geometry = ShapeGeometry::new(outline, size)?;
            clip = Some(geometry.clip);
            Ok::<_, ViewError>(geometry.input_cells)
        })?;
        if let Some(path) = clip {
            let outline = surface.shape.clips_paint().then_some(path);
            (self.actors.renderer).send(RendererInput::Outline {
                surface: id,
                outline,
            })?;
        }
        Ok(shape_changes)
    }

    /// Makes the renderer draw the newest built scene of surface `id` with its surface state: the frame composed for
    /// the surface, or else a frame for `timing` that measures zero view CPU time. The renderer reports the frame once
    /// it is drawn.
    fn render_surface(&mut self, id: SurfaceId, timing: FrameTime) -> Result<(), ViewError> {
        let scale = (self.records.open_scale(id)).ok_or(ViewError::SurfaceNotOpen(id))?;
        let (surface, _, viewport) = open_parts(&mut self.surfaces, &mut self.records, id)?;
        let start = match surface.frame {
            FrameProgress::Composed(composed) => composed,
            FrameProgress::Idle | FrameProgress::Drawing { .. } => FrameStart {
                wall: Instant::now(),
                view_cpu: Some(Duration::ZERO),
                timing,
                viewport,
                scale,
            },
        };
        let built = self.actors.scene.built(id);
        let panel = self.panels.requests(id, &built.state);
        surface.frame = FrameProgress::Drawing { start, panel };
        self.actors.renderer.send(RendererInput::Render {
            surface: id,
            time: start.timing,
            viewport: start.viewport,
            scale: start.scale,
            built,
        })?;
        Ok(())
    }

    /// Records the frame drawn by the renderer, reports it to the observers of the app while the surface is open, and
    /// reports it to the frame scheduler. The scheduler receives the cost of the draw alone, without the wait for the
    /// scene builder.
    pub(super) fn rendered(&mut self, rendered: &Rendered) {
        let Rendered {
            surface: id,
            time,
            viewport,
            scale,
            drawn,
            contents,
            next_change,
            cost,
            cpu_time,
        } = *rendered;
        let outcome = drawn.outcome;
        if self.records.is_open(id)
            && let Some(surface) = self.surfaces.get_mut(&id)
        {
            surface.last_frame = Some(LastFrame {
                viewport,
                scale,
                contents,
            });
            if let FrameProgress::Drawing { start, panel } = surface.frame
                && start.timing.sequence == time.sequence
            {
                surface.frame = FrameProgress::Idle;
                if outcome == FrameOutcome::Shown
                    && let Some(requests) = panel
                {
                    let content = PanelContent {
                        size: viewport.size,
                        input: surface.shape.accepts_input(),
                    };
                    self.panels.shown(id, content, requests);
                }
                self.report_frame(id, drawn, start, cpu_time);
            }
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

    /// Reports the frame of surface `id` to the observers of the app. A frame that was not shown invalidates the view.
    /// `render_cpu` is the CPU time that the renderer spent on the frame.
    fn report_frame(
        &mut self,
        id: SurfaceId,
        drawn: Drawn,
        start: FrameStart,
        render_cpu: Option<Duration>,
    ) {
        let Ok((surface, view, viewport)) = open_parts(&mut self.surfaces, &mut self.records, id)
        else {
            return;
        };
        if let Some(observer) = &mut self.observers.view {
            observer(id, start.timing, viewport, view);
        }
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
                    scene: drawn.scene,
                    gpu: drawn.gpu,
                }),
            });
        }
    }
}

/// The surface `id` with the view and the viewport shown on it, while the surface is open.
pub(super) fn open_parts<'a>(
    surfaces: &'a mut BTreeMap<SurfaceId, ViewSurface>,
    records: &'a mut SurfaceRecords,
    id: SurfaceId,
) -> Result<(&'a mut ViewSurface, &'a mut View, Viewport), ViewError> {
    let viewport = records.open_viewport(id);
    let found = (surfaces.get_mut(&id))
        .zip(records.open_view_mut(id))
        .zip(viewport);
    let ((surface, view), viewport) = found.ok_or(ViewError::SurfaceNotOpen(id))?;
    Ok((surface, view, viewport))
}
