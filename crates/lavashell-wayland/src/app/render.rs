// SPDX-License-Identifier: GPL-3.0-only
use super::shape::ShapeGeometry;
use super::{Platform, Role, open_surface, send_surface_state};
use crate::Error;
use lavashell_core::frame::{FrameReport, FrameStage, FrameTime, Submission};
use lavashell_core::geometry::Viewport;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_renderer::{Drawn, FrameOutcome, PresentFrame};
use lavashell_runtime::{ActionBatch, PanelContent, SceneSink};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct FrameStart {
    wall: Instant,
    cpu: Option<Duration>,
    timing: FrameTime,
    viewport: Viewport,
}

/// Whether the event loop renders the surfaces again at once or waits for the next event.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NextRender {
    Immediately,
    AfterEvent,
}

impl Platform {
    pub(super) fn render(&mut self) -> Result<NextRender, Error> {
        // Cancelling a transfer calls the app callback with the view of the surface, so it runs before the surface
        // retires.
        self.cancel_closed_transfers();
        self.execute_surface_requests();
        self.submit_transfers();
        self.accessibility_updates();
        let mut actions = self.config.action_queue.take_batch();
        self.begin_frame(&actions)?;
        let mut frame_notified = false;
        let mut next = NextRender::AfterEvent;
        for id in self.composition_order() {
            let wanted = self.wants_frame(id, &actions);
            if wanted == Wanted::Idle {
                continue;
            }
            let start = self.prepare_surface(id, wanted, &mut actions, &mut frame_notified)?;
            let drawn = self.draw_surface(id, start)?;
            if self.report_frame(id, drawn, start)? == NextRender::Immediately {
                next = NextRender::Immediately;
            }
        }
        self.config.action_queue.requeue(actions);
        // View actions queued during composition run in another frame. A configured surface that waits for no
        // frame callback can draw that frame at once.
        if self.config.action_queue.has_pending()
            && (self.surfaces.iter()).any(|(&id, surface)| {
                self.surface_records.is_configured(id) && !surface.frame_callback_pending
            })
        {
            next = NextRender::Immediately;
        }
        self.connection.flush().map_err(Error::Flush)?;
        Ok(next)
    }

    fn wants_frame(&self, id: SurfaceId, actions: &ActionBatch) -> Wanted {
        let continuous = self.config.tick.is_some()
            || self.config.on_frame.is_some()
            || self.renderer.shows_scene();
        let (Some(surface), Some(view)) =
            (self.surfaces.get(&id), self.surface_records.open_view(id))
        else {
            return Wanted::Idle;
        };
        if !self.surface_records.is_configured(id) || surface.frame_callback_pending {
            Wanted::Idle
        } else if continuous || surface.needs_visual_frame(view) || actions.has_actions_for(id) {
            Wanted::View
        } else if surface.moving(self.start.elapsed()) {
            Wanted::Scene
        } else {
            Wanted::Idle
        }
    }

    /// Advances the scene source in the viewport of the first configured surface, so the scene work runs on the GPU
    /// while the views are composed. Calls the tick callback of the app when at least one surface draws.
    fn begin_frame(&mut self, actions: &ActionBatch) -> Result<(), Error> {
        let viewport = (self.surfaces.keys())
            .find(|&&id| self.surface_records.is_configured(id))
            .and_then(|&id| self.surface_records.open_viewport(id));
        if let Some(viewport) = viewport {
            (self.renderer.advance_scene(viewport)).map_err(Error::Render)?;
        }
        if (self.surfaces.keys()).any(|&id| self.wants_frame(id, actions) != Wanted::Idle) {
            let now = self.start.elapsed();
            if let Some(tick) = self.config.tick.as_mut() {
                tick(now.saturating_sub(self.last_tick));
            }
            self.last_tick = now;
        }
        Ok(())
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

    /// Composes the view of surface `id` with its queued actions, sends the state of the surface for the frame with
    /// it, and builds the frame.
    fn prepare_surface(
        &mut self,
        id: SurfaceId,
        wanted: Wanted,
        actions: &mut ActionBatch,
        frame_notified: &mut bool,
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
            timing: surface.clock.sample(self.start.elapsed()),
            viewport,
        };
        if !*frame_notified {
            if let Some(callback) = self.config.on_frame.as_mut() {
                callback(start.timing);
            }
            *frame_notified = true;
        }
        let mut changed = false;
        if wanted == Wanted::View {
            let mut frame = |view: &mut View, sink: &mut SceneSink| {
                view.frame_into_at(viewport, self.renderer.shaper(), start.timing, sink)
            };
            changed = view.needs_visual_frame();
            surface.compose(id, view, &mut self.choreographers, &mut frame)?;
            actions.run(id, view);
            if view.needs_visual_frame() {
                changed = true;
                surface.compose(id, view, &mut self.choreographers, &mut frame)?;
            }
        }
        // The composed view can change the size of a panel that fits its content, or the panel layout can
        // change its margins. The geometry for that is sent with this frame, so that the compositor applies it with
        // the buffer of this frame.
        if changed {
            self.panels.measure(id, view, self.renderer.shaper())?;
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
            (self.renderer.set_outline(id, outline)).map_err(Error::Render)?;
        }
        let sink = self.choreographers.sink(id);
        send_surface_state(sink, &self.panels, id, shape_changes);
        surface.build(id, view, &mut self.choreographers)?;
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
            state: &surface.state,
        };
        let snapshot = surface.builder.snapshot();
        let drawn = (self.renderer.draw(id, frame, &snapshot)).map_err(Error::Draw)?;
        if drawn.outcome == FrameOutcome::Shown {
            let content = PanelContent {
                size: viewport.size,
                input: surface.shape.accepts_input(),
            };
            self.panels.shown(id, content);
        }
        Ok(drawn)
    }

    /// Reports the frame of surface `id` to the observers of the app. Returns whether the loop renders again
    /// at once. A stale swapchain is replaced by the next draw, so that draw follows at once. A pending frame
    /// waits for a configure or a buffer release sent by the compositor. A shown frame waits for its frame
    /// callback.
    fn report_frame(
        &mut self,
        id: SurfaceId,
        drawn: Drawn,
        start: FrameStart,
    ) -> Result<NextRender, Error> {
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
            return Ok(if drawn.outcome == FrameOutcome::Stale {
                NextRender::Immediately
            } else {
                NextRender::AfterEvent
            });
        }
        view.record_presentation(start.timing.presentation);
        surface.frame_callback_pending = true;
        let render_cost = start.wall.elapsed();
        surface.clock.render_cost(render_cost);
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
        Ok(NextRender::AfterEvent)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Wanted {
    View,
    /// The view is unchanged, but motions in the scene tree of the surface still play.
    Scene,
    Idle,
}

/// The CPU time of the calling thread, or `None` when the clock reports a negative time.
fn thread_cpu_time() -> Option<Duration> {
    let stamp = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
    Duration::try_from(stamp).ok()
}
