// SPDX-License-Identifier: GPL-3.0-only
use super::shape::{ShapeUpdate, SurfaceShape};
use super::{Platform, Role, layer, open_surface};
use crate::Error;
use lavashell_core::frame::{FrameReport, FrameStage, FrameTime, Submission};
use lavashell_core::geometry::Viewport;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_renderer::{Drawn, FrameOutcome, PresentFrame};
use lavashell_runtime::{ActionBatch, FrameKind, SceneSink, SchedulerInput, ViewState};
use smithay_client_toolkit as sctk;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct FrameStart {
    wall: Instant,
    cpu: Option<Duration>,
    timing: FrameTime,
    viewport: Viewport,
}

impl Platform {
    /// Draws the frame started by the frame scheduler for each surface, in composition order. A surface reports what
    /// the view needs right before the surface draws, so the scheduler samples the frame at that time. After the
    /// draws, every surface reports again with the deadline after composing, and each drawn surface reports the
    /// rendered frame.
    pub(super) fn render(&mut self) -> Result<(), Error> {
        // Cancelling a transfer calls the app callback with the view of the surface, so it runs before the surface
        // retires.
        self.cancel_closed_transfers();
        self.execute_surface_requests();
        self.submit_transfers();
        self.accessibility_updates();
        let mut actions = self.config.action_queue.take_batch();
        let order = self.composition_order();
        self.begin_frame()?;
        let mut drawn = Vec::new();
        for &id in &order {
            if !self.surface_records.is_open(id) {
                let _closed_frame = self.started_frames.remove(&id);
                continue;
            }
            self.want_frame(id, actions.has_actions_for(id));
            let Some((kind, timing)) = self.started_frames.remove(&id) else {
                continue;
            };
            if drawn.is_empty() {
                self.notify_frame(timing);
            }
            let start = self.prepare_surface(id, kind, timing, &mut actions)?;
            let frame = self.draw_surface(id, start)?;
            let outcome = frame.outcome;
            let cost = self.report_frame(id, frame, start)?;
            drawn.push((id, timing, outcome, cost));
        }
        self.config.action_queue.requeue(actions);
        // View actions queued during composition run in another frame, started at once by a surface that is ready.
        for &id in &order {
            let queued = self.config.action_queue.has_pending_for(id);
            self.want_frame(id, queued);
        }
        for (id, timing, outcome, cost) in drawn {
            let next_change = (self.surfaces.get(&id))
                .and_then(|surface| surface.builder.next_change(timing.presentation));
            self.schedule(SchedulerInput::Published {
                surface: id,
                next_change,
            });
            self.schedule(SchedulerInput::Rendered {
                surface: id,
                frame: timing,
                outcome,
                cost,
            });
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
    /// frame of a render.
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

    /// Composes the view of surface `id` for `timing` with its queued actions, and updates the role, the shape and
    /// the blur region of the surface for the frame.
    fn prepare_surface(
        &mut self,
        id: SurfaceId,
        kind: FrameKind,
        timing: FrameTime,
        actions: &mut ActionBatch,
    ) -> Result<FrameStart, Error> {
        let (surface, view, viewport) =
            open_surface(&mut self.surfaces, &mut self.surface_records, id)?;
        let Viewport { size, density } = viewport;
        let start = FrameStart {
            wall: Instant::now(),
            cpu: self
                .config
                .observer
                .as_ref()
                .and_then(|_| thread_cpu_time()),
            timing,
            viewport,
        };
        let mut changed = false;
        if kind == FrameKind::Compose {
            let mut frame = |view: &mut View, sink: &mut SceneSink| {
                view.frame_into_at(viewport, self.renderer.shaper(), start.timing, sink)
            };
            changed = view.needs_visual_frame();
            surface.show(id, view, &mut self.choreographers, &mut frame)?;
            actions.run(id, view);
            if view.needs_visual_frame() {
                changed = true;
                surface.show(id, view, &mut self.choreographers, &mut frame)?;
            }
        }
        // The composed view can change the size of a layer that fits its content, or the layer layout can
        // change its margins. The layer-shell requests for that are queued now, so that the compositor
        // applies them with the buffer of this frame.
        if let Role::Layer(layer) = &mut surface.role {
            if changed && let Some(proposal) = layer.content_proposal() {
                let content =
                    lavashell_runtime::measure_content(view, proposal, self.renderer.shaper())?;
                layer.content_measured(content);
            }
            layer.update();
        }
        if let Some(bridge) = &mut surface.accessibility {
            bridge.publish(view.accessibility(), false);
        }
        let shape_changed = match &mut surface.shape {
            Some(shape) => shape.update(size, density, &surface.wl, &self.compositor)?,
            None => ShapeUpdate::Unchanged,
        } == ShapeUpdate::Changed;
        if shape_changed {
            let outline = surface.shape.as_ref().and_then(SurfaceShape::path).cloned();
            (self.renderer.set_outline(id, outline)).map_err(Error::Render)?;
        }
        if let Some(background) = &surface.background
            && (shape_changed || surface.blur_size != Some(size))
        {
            let region = if let Some(shape) = &surface.shape {
                shape.region(&self.compositor)?
            } else {
                let region =
                    sctk::compositor::Region::new(&self.compositor).map_err(Error::CreateRegion)?;
                let (width, height) =
                    crate::logical::protocol_size(size).ok_or(Error::SurfaceSize(size))?;
                region.add(0, 0, width, height);
                region
            };
            background.set_blur_region(Some(region.wl_region()));
            surface.blur_size = Some(size);
        }
        Ok(start)
    }

    /// Draws the frame of surface `id` at the time and in the viewport of `start`, from the snapshot of its scene
    /// builder.
    fn draw_surface(&mut self, id: SurfaceId, start: FrameStart) -> Result<Drawn, Error> {
        let FrameStart {
            timing, viewport, ..
        } = start;
        let (surface, _, _) = open_surface(&mut self.surfaces, &mut self.surface_records, id)?;
        let frame = PresentFrame {
            time: timing,
            viewport,
            present_at: None,
        };
        let snapshot = surface.builder.snapshot();
        let drawn = (self.renderer.draw(id, frame, &snapshot)).map_err(Error::Draw)?;
        if let Role::Layer(layer) = &mut surface.role {
            let input = (surface.shape.as_ref()).is_none_or(SurfaceShape::accepts_input);
            let content = layer::Content {
                size: viewport.size,
                input,
            };
            layer.presented(drawn.outcome, content);
        }
        Ok(drawn)
    }

    /// Reports the frame of surface `id` to the observers of the app, and returns the render cost of the frame. A
    /// frame that was not shown invalidates the view.
    fn report_frame(
        &mut self,
        id: SurfaceId,
        drawn: Drawn,
        start: FrameStart,
    ) -> Result<Duration, Error> {
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
        let render_cost = start.wall.elapsed();
        if drawn.outcome != FrameOutcome::Shown {
            view.invalidate();
            return Ok(render_cost);
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
                    render_cost,
                    cpu_time: now.saturating_sub(started),
                    view: view.stats(),
                    render: drawn.stats,
                    scene: drawn.scene,
                    gpu: drawn.gpu,
                }),
            });
        }
        Ok(render_cost)
    }
}

/// The CPU time of the calling thread, or `None` when the clock reports a negative time.
fn thread_cpu_time() -> Option<Duration> {
    let stamp = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
    Duration::try_from(stamp).ok()
}
