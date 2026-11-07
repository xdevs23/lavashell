// SPDX-License-Identifier: GPL-3.0-only
use super::shape::ShapeGeometry;
use super::{AppConfig, LastFrame, Platform, Role, open_surface, send_frame, send_surface_state};
use crate::Error;
use lavashell_core::frame::{FrameReport, FrameStage, FrameTime, Submission};
use lavashell_core::geometry::Viewport;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_renderer::{Drawn, FrameOutcome};
use lavashell_runtime::{
    ActionBatch, CpuStamp, FrameKind, FrameSequence, PanelContent, PanelRequests, Rendered,
    RendererInput, SceneSink, SchedulerInput, ViewState,
};
use std::time::{Duration, Instant};

/// When a frame started on the wall clock, the CPU time that the views thread spent on the frame, and the time and
/// the viewport of the frame.
#[derive(Clone, Copy)]
pub(super) struct FrameStart {
    wall: Instant,
    /// Measured only while an observer watches the frames.
    view_cpu: Option<Duration>,
    timing: FrameTime,
    viewport: Viewport,
}

/// How far the frame of a surface got between its compose and the report of its draw.
#[derive(Clone, Copy)]
pub(super) enum FrameProgress {
    Idle,
    /// The view composed the frame. The frame draws in the viewport of the compose, and observers measure it from the
    /// start of the compose.
    Composed(FrameStart),
    /// The renderer draws the frame. A shown frame applies `panel`.
    Drawing {
        start: FrameStart,
        panel: Option<PanelRequests>,
    },
}

impl AppConfig {
    /// Reads the CPU clock of the thread only while an observer watches the frames.
    fn cpu_stamp(&self) -> Option<CpuStamp> {
        self.observer.as_ref().and_then(|_| CpuStamp::now())
    }
}

impl Platform {
    /// Makes the renderer commit each surface state in `state_commits` without a frame, then runs the frame started by
    /// the frame scheduler for each surface, in composition order. A compose frame sends the frames of the view and
    /// the surface state to the scene pipeline, and a render frame makes the renderer draw the newest scene of the
    /// surface. Before the frame of a surface runs, the surface reports what the view needs. The scheduler samples the
    /// frame at that time. After the frames, every surface reports again with the deadline after composing. Returns
    /// an error after the scene pipeline or the renderer has stopped.
    pub(super) fn render(&mut self) -> Result<(), Error> {
        // Cancelling a transfer calls the app callback with the view of the surface, so it runs before the surface
        // retires.
        self.cancel_closed_transfers();
        self.execute_surface_requests()?;
        self.stops.check()?;
        self.commit_built_states()?;
        self.submit_transfers();
        self.accessibility_updates();
        let mut actions = self.config.action_queue.take_batch();
        let order = self.composition_order();
        self.advance_scene()?;
        let mut composed = false;
        for &id in &order {
            if !self.surface_records.is_open(id) {
                let _closed_frame = self.started_frames.remove(&id);
                continue;
            }
            self.want_frame(id, actions.has_actions_for(id));
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
                FrameKind::Render => self.render_surface(id, timing)?,
            }
        }
        self.config.action_queue.requeue(actions);
        // View actions queued during composition run in another frame, started at once by a surface that is ready.
        for &id in &order {
            let queued = self.config.action_queue.has_pending_for(id);
            self.want_frame(id, queued);
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

    /// Makes the renderer advance the scene source in the viewport of the first configured surface while the source
    /// is due, so the scene work runs on the GPU while the views are composed.
    fn advance_scene(&mut self) -> Result<(), Error> {
        let due = (self.scene_deadline).is_some_and(|deadline| deadline <= Instant::now());
        if due
            && let Some(viewport) = (self.surfaces.keys())
                .find(|&&id| self.surface_records.is_configured(id))
                .and_then(|&id| self.surface_records.open_viewport(id))
        {
            self.renderer.send(RendererInput::AdvanceScene(viewport))?;
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

    /// Composes the view of surface `id` for `timing` with its queued actions, and sends the frames of the view and
    /// then the surface state for the frame to the scene pipeline. The frame renders once the scene builder built the
    /// state. When the state does not end a frame, because the sink waits for the whole scene, the frame renders at
    /// once.
    fn compose_surface(
        &mut self,
        id: SurfaceId,
        timing: FrameTime,
        actions: &mut ActionBatch,
    ) -> Result<(), Error> {
        let wall = Instant::now();
        let cpu = self.config.cpu_stamp();
        let (surface, view, viewport) =
            open_surface(&mut self.surfaces, &mut self.surface_records, id)?;
        let Viewport { size, density } = viewport;
        let sequence = Some(FrameSequence(timing.sequence));
        let mut frame = |view: &mut View, sink: &mut SceneSink<'_>| {
            view.frame_into_at(viewport, &mut self.shaper, timing, sink)
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
            self.panels.measure(id, view, &mut self.shaper)?;
        }
        surface.send_keyboard(id, self.panels.update(id))?;
        if let Some(bridge) = &mut surface.accessibility {
            bridge.publish(view.accessibility(), false);
        }
        let mut clip = None;
        let shape_changes = surface.shape.update(size, density, |outline| {
            let geometry = ShapeGeometry::new(outline, size, density)?;
            clip = Some(geometry.clip);
            Ok::<_, Error>(geometry.input_cells)
        })?;
        if let Some(path) = clip {
            let outline = surface.shape.clips_paint().then_some(path);
            (self.renderer).send(RendererInput::Outline {
                surface: id,
                outline,
            })?;
        }
        let sink = self.scene.sink(id, sequence);
        let ended = send_surface_state(sink, &self.panels, id, shape_changes)?;
        surface.frame = FrameProgress::Composed(FrameStart {
            wall,
            view_cpu: cpu.and_then(CpuStamp::elapsed),
            timing,
            viewport,
        });
        if !ended {
            self.schedule(SchedulerInput::Built {
                surface: id,
                sequence,
            });
        }
        Ok(())
    }

    /// Makes the renderer draw the newest built scene of surface `id`: the frame composed for the surface, or else a
    /// frame for `timing` that measures zero view CPU time. The renderer reports the frame once it is drawn.
    fn render_surface(&mut self, id: SurfaceId, timing: FrameTime) -> Result<(), Error> {
        let (surface, _, viewport) =
            open_surface(&mut self.surfaces, &mut self.surface_records, id)?;
        let start = match surface.frame {
            FrameProgress::Composed(composed) => composed,
            FrameProgress::Idle | FrameProgress::Drawing { .. } => FrameStart {
                wall: Instant::now(),
                view_cpu: Some(Duration::ZERO),
                timing,
                viewport,
            },
        };
        let built = self.scene.built(id);
        let panel = self.panels.requests(id, &built.state);
        surface.frame = FrameProgress::Drawing { start, panel };
        self.renderer.send(RendererInput::Render {
            surface: id,
            time: start.timing,
            viewport: start.viewport,
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
            drawn,
            contents,
            next_change,
            cost,
            cpu_time,
        } = *rendered;
        let outcome = drawn.outcome;
        if self.surface_records.is_open(id)
            && let Some(surface) = self.surfaces.get_mut(&id)
        {
            surface.last_frame = Some(LastFrame { viewport, contents });
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
        let Ok((surface, view, viewport)) =
            open_surface(&mut self.surfaces, &mut self.surface_records, id)
        else {
            return;
        };
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
            return;
        }
        view.record_presentation(start.timing.presentation);
        if let Some(observer) = &mut self.config.observer
            && let Some(view_cpu) = start.view_cpu
            && let Some(render_cpu) = render_cpu
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
