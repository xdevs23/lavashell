// SPDX-License-Identifier: GPL-3.0-only
use super::shape::ShapeGeometry;
use super::{AppConfig, Platform, Role, actor_link, open_surface, send_frame, send_surface_state};
use crate::Error;
use lavashell_actor::Closed;
use lavashell_core::frame::{FrameReport, FrameStage, FrameTime, Submission};
use lavashell_core::geometry::Viewport;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_renderer::{Drawn, FrameOutcome, PresentFrame};
use lavashell_runtime::{
    ActionBatch, BuiltSurface, FrameKind, FrameSequence, PanelContent, SceneSink, SchedulerInput,
    ViewState,
};
use std::mem;
use std::time::{Duration, Instant};

/// When a frame started on the wall clock and on the CPU clock of the thread, with the time and the viewport of the
/// frame.
#[derive(Clone, Copy)]
pub(super) struct FrameStart {
    wall: Instant,
    cpu: Option<Duration>,
    timing: FrameTime,
    viewport: Viewport,
}

/// What the next render of a surface draws.
#[derive(Clone, Copy)]
pub(super) enum NextDraw {
    /// The frame that the view composed. It draws in the viewport of the compose, and observers measure it from the
    /// start of the compose.
    Composed(FrameStart),
    /// The newest scene in the current viewport, without a compose, because the scene moved or was built late.
    SceneOnly,
}

impl FrameStart {
    fn new(timing: FrameTime, viewport: Viewport, cpu: Option<Duration>) -> Self {
        Self {
            wall: Instant::now(),
            cpu,
            timing,
            viewport,
        }
    }
}

impl AppConfig {
    /// Starts the frame for `timing` in `viewport`. Reads the CPU clock of the thread only while an observer watches
    /// the frames.
    fn start_frame(&self, timing: FrameTime, viewport: Viewport) -> FrameStart {
        let cpu = self.observer.as_ref().and_then(|_| thread_cpu_time());
        FrameStart::new(timing, viewport, cpu)
    }
}

impl Platform {
    /// Runs the frame started by the frame scheduler for each surface, in composition order. Returns an error after
    /// the scene pipeline or the frame scheduler has stopped. When an actor failed, the error is its failure.
    pub(super) fn render(&mut self) -> Result<(), Error> {
        if self.scheduler_stopped {
            return Err(self.stopped_scheduler_error());
        }
        match self.run_started_frames() {
            Err(Error::SceneStopped) => Err(self.stopped_scene_error()),
            ran => ran,
        }
    }

    /// Joins the threads of the stopped scene pipeline, and returns the failure of the actor that stopped it, or
    /// `SceneStopped` when neither actor failed.
    fn stopped_scene_error(&mut self) -> Error {
        actor_link::stopped(self.scene.stop(), Error::SceneStopped)
    }

    /// Runs the frame started by the frame scheduler for each surface, in composition order. A compose frame sends the
    /// frames of the view to the scene pipeline, and a render frame draws the newest scene of the surface. After the
    /// frames, every surface reports what its view needs, with the deadline after composing, and the scheduler starts
    /// the next frame from that report. Each drawn surface reports the rendered frame.
    fn run_started_frames(&mut self) -> Result<(), Error> {
        self.execute_surface_requests()?;
        if self.scene_stopped {
            return Err(Error::SceneStopped);
        }
        self.commit_built_states()?;
        self.clipboard.transfers.submit(&self.surface_records);
        self.submit_transfers();
        self.accessibility_updates();
        let mut actions = self.config.action_queue.take_batch();
        let order = self.composition_order();
        self.begin_frame()?;
        let mut composed = false;
        let mut reports = Vec::new();
        for &id in &order {
            if !self.surface_records.is_open(id) {
                let _closed_frame = self.started_frames.remove(&id);
                continue;
            }
            let Some((kind, timing)) = self.started_frames.remove(&id) else {
                continue;
            };
            match kind {
                FrameKind::Compose => {
                    if !composed {
                        self.notify_frame(timing);
                        composed = true;
                    }
                    self.compose_surface(id, timing, &mut actions)?;
                }
                FrameKind::Render => reports.extend(self.render_surface(id, timing)?),
            }
        }
        self.config.action_queue.requeue(actions);
        // View actions queued during composition run in another frame, started at once by a surface that is ready.
        for &id in &order {
            let queued = self.config.action_queue.has_pending_for(id);
            self.want_frame(id, queued);
        }
        for report in reports {
            self.schedule(report);
        }
        self.connection.flush().map_err(Error::Flush)?;
        Ok(())
    }

    /// Reports to the frame scheduler what the view of surface `id` needs, while the surface is open. The view of a
    /// configured surface changed when it needs a visual frame or `actions_queued` is true.
    fn want_frame(&mut self, id: SurfaceId, actions_queued: bool) {
        let (Some(surface), Some(view)) =
            (self.surfaces.get(&id), self.surface_records.open_view(id))
        else {
            return;
        };
        let state = if !self.surface_records.is_configured(id) {
            ViewState::Unconfigured
        } else if surface.needs_visual_frame(view) || actions_queued {
            ViewState::Changed
        } else {
            ViewState::Unchanged
        };
        let deadline = view.next_deadline();
        self.schedule(SchedulerInput::Wants {
            surface: id,
            view: state,
            deadline,
        });
    }

    /// Advances the scene source in the viewport of the first configured surface, so the scene work runs on the GPU
    /// while the views are composed.
    fn begin_frame(&mut self) -> Result<(), Error> {
        let viewport = (self.surfaces.keys())
            .find(|&&id| self.surface_records.is_configured(id))
            .and_then(|&id| self.surface_records.open_viewport(id));
        if let Some(viewport) = viewport {
            (self.renderer.advance_scene(viewport)).map_err(Error::Render)?;
        }
        Ok(())
    }

    /// Calls the tick callback of the app, and the frame callback of the app with `timing`, the timing of the first
    /// frame composed in a render.
    fn notify_frame(&mut self, timing: FrameTime) {
        let now = self.start.elapsed();
        if let Some(tick) = self.config.tick.as_mut() {
            tick(now.saturating_sub(self.last_tick));
        }
        self.last_tick = now;
        if let Some(callback) = self.config.on_frame.as_mut() {
            callback(timing);
        }
    }

    fn composition_order(&self) -> Vec<SurfaceId> {
        let mut order: Vec<_> = (self.surfaces.iter())
            .map(|(&id, surface)| match &surface.role {
                Role::Layer(layer) => (layer.composition_order, id),
                _ => (0, id),
            })
            .collect();
        order.sort_unstable();
        order.into_iter().map(|(_, id)| id).collect()
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
    ) -> Result<(), Error> {
        let (surface, view, viewport) =
            open_surface(&mut self.surfaces, &mut self.surface_records, id)?;
        let Viewport { size, density } = viewport;
        let start = self.config.start_frame(timing, viewport);
        let mut frame = |view: &mut View, sink: &mut SceneSink<'_>| {
            view.frame_into_at(viewport, self.renderer.shaper(), timing, sink)
        };
        let mut changed = view.needs_visual_frame();
        send_frame(view, self.scene.sink(id, None), &mut frame)?;
        actions.run(id, view);
        if view.needs_visual_frame() {
            changed = true;
            send_frame(view, self.scene.sink(id, None), &mut frame)?;
        }
        // The composed view can change the size of a panel that fits its content, or the panel layout can
        // change its margins. The geometry for that is sent with this frame, so that the compositor applies it with
        // the buffer of this frame.
        if changed {
            self.panels.measure(id, view, self.renderer.shaper())?;
        }
        surface.send_keyboard(id, self.panels.update(id))?;
        let mut clip = None;
        let shape_changes = surface.shape.update(size, density, |outline| {
            let geometry = ShapeGeometry::new(outline, size, density)?;
            clip = Some(geometry.clip);
            Ok::<_, Error>(geometry.input_cells)
        })?;
        if let Some(path) = clip {
            let outline = surface.shape.clips_paint().then_some(path);
            (self.renderer.set_outline(id, outline)).map_err(Error::Render)?;
        }
        let sequence = Some(FrameSequence(timing.sequence));
        let sink = self.scene.sink(id, sequence);
        let sent = send_surface_state(sink, &self.panels, id, shape_changes)
            .map_err(|Closed| Error::SceneStopped)?;
        surface.next_draw = NextDraw::Composed(start);
        self.publish_accessibility(id);
        if !sent {
            self.schedule(SchedulerInput::Built {
                surface: id,
                sequence,
            });
        }
        Ok(())
    }

    /// Draws the newest scene of surface `id` with its surface state as the frame for `timing`, and returns the reports
    /// of the frame for the frame scheduler. The scheduler receives the cost of the render alone, without the wait for
    /// the scene builder.
    fn render_surface(
        &mut self,
        id: SurfaceId,
        timing: FrameTime,
    ) -> Result<[SchedulerInput; 2], Error> {
        let rendering = Instant::now();
        let (surface, _, viewport) =
            open_surface(&mut self.surfaces, &mut self.surface_records, id)?;
        let start = match mem::replace(&mut surface.next_draw, NextDraw::SceneOnly) {
            NextDraw::Composed(composed) => composed,
            NextDraw::SceneOnly => self.config.start_frame(timing, viewport),
        };
        let built = self.scene.built(id);
        let drawn = self.draw_surface(id, start, &built)?;
        let outcome = drawn.outcome;
        let cost = rendering.elapsed();
        self.report_frame(id, drawn, start)?;
        let published = SchedulerInput::Published {
            surface: id,
            next_change: built.scene.next_change(timing.presentation),
        };
        let rendered = SchedulerInput::Rendered {
            surface: id,
            frame: timing,
            outcome,
            cost,
        };
        Ok([published, rendered])
    }

    /// Commits the surface state of each surface whose state the scene builder built without a frame.
    fn commit_built_states(&mut self) -> Result<(), Error> {
        for (&id, surface) in &mut self.surfaces {
            if mem::take(&mut surface.uncommitted_state) {
                let built = self.scene.built(id);
                (self.renderer.commit_state(id, &built.state)).map_err(Error::CommitState)?;
            }
        }
        Ok(())
    }

    /// Draws `built` as the frame of surface `id` at the time and in the viewport of `start`.
    fn draw_surface(
        &mut self,
        id: SurfaceId,
        start: FrameStart,
        built: &BuiltSurface,
    ) -> Result<Drawn, Error> {
        let (surface, _, _) = open_surface(&mut self.surfaces, &mut self.surface_records, id)?;
        let frame = PresentFrame {
            time: start.timing,
            viewport: start.viewport,
            present_at: None,
            state: &built.state,
        };
        let drawn = (self.renderer.draw(id, frame, &built.scene)).map_err(Error::Draw)?;
        if drawn.outcome == FrameOutcome::Shown {
            let content = PanelContent {
                size: start.viewport.size,
                input: surface.shape.accepts_input(),
            };
            self.panels.shown(id, content);
        }
        Ok(drawn)
    }

    /// Reports the frame of surface `id` to the observers of the app. A frame that was not shown invalidates the view.
    fn report_frame(
        &mut self,
        id: SurfaceId,
        drawn: Drawn,
        start: FrameStart,
    ) -> Result<(), Error> {
        let (surface, view, viewport) =
            open_surface(&mut self.surfaces, &mut self.surface_records, id)?;
        if let Some(observer) = &mut self.config.view_observer {
            observer(id, viewport, view);
        }
        if let Some(info) = drawn.output
            && surface.output != Some(info)
        {
            surface.output = Some(info);
            if let Some(observer) = &mut self.config.output_observer {
                observer(id, info);
            }
        }
        if drawn.outcome != FrameOutcome::Shown {
            view.invalidate();
            return Ok(());
        }
        view.record_presentation(start.timing.presentation);
        if let Some(observer) = &mut self.config.observer
            && let Some(started) = start.cpu
            && let Some(now) = thread_cpu_time()
        {
            observer(FrameReport {
                surface: id,
                timing: start.timing,
                stage: FrameStage::Submitted,
                observed: self.start.elapsed(),
                presentation_time: None,
                metrics: None,
                flags: None,
                submission: Some(Submission {
                    render_cost: start.wall.elapsed(),
                    cpu_time: now.saturating_sub(started),
                    view: view.stats(),
                    render: drawn.stats,
                    scene: drawn.scene,
                    gpu: drawn.gpu,
                }),
            });
        }
        Ok(())
    }
}

/// The CPU time of the calling thread, or `None` when the clock reports a negative time.
fn thread_cpu_time() -> Option<Duration> {
    let stamp = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
    Duration::try_from(stamp).ok()
}
