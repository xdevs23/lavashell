// SPDX-License-Identifier: GPL-3.0-only
use super::scene::SceneRuntime;
use super::shape::{ShapeUpdate, SurfaceShape};
use super::view_actions::PendingAction;
use super::{
    Density, Dp, Duration, FrameCallbackData, FrameOutcome, FrameReport, FrameStage, FrameTime,
    Instant, Platform, Rect, Role, SceneTree, View, Viewport, layer, presentation, sctk,
};
use crate::Error;
use lavashell_core::frame::Submission;
use lavashell_renderer::effect_registry::QualityChange;
use lavashell_renderer::vulkan::{CompletedScene, SceneImage};
use lavashell_renderer::{SurfaceContents, SurfaceTarget};
use std::sync::Arc;

#[derive(Clone, Copy)]
struct FrameStart {
    wall: Instant,
    cpu: Option<Duration>,
    timing: FrameTime,
}

/// Whether the event loop renders the surfaces again at once or waits for the next event.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NextRender {
    Immediately,
    AfterEvent,
}

impl Platform {
    pub(super) fn render(&mut self) -> Result<NextRender, Error> {
        self.submit_transfers();
        self.accessibility_updates();
        let mut actions = self.config.view_actions.drain();
        let mut scene = self.begin_frame(&actions)?;
        let mut frame_notified = false;
        let mut next = NextRender::AfterEvent;
        for index in self.composition_order() {
            let surface = &mut self.surfaces[index];
            if surface.lifecycle.is_closing() {
                self.paint.forget_effect_surface(surface.id);
                surface.retire();
            }
            let wanted = self.wants_frame(index, &actions);
            if wanted == Wanted::Idle {
                continue;
            }
            let start = self.prepare_surface(index, wanted, &mut actions, &mut frame_notified)?;
            let frames = self.scene_frames_in_flight();
            if let Some(runtime) = &mut self.scene {
                // Picks up a scene that completed while the views were composed. The UI queue acquires that
                // scene before the draw below reads it.
                let viewport = self.surfaces[index].logical_viewport();
                runtime.tick(&mut self.gpu, self.start.elapsed(), viewport, frames)?;
                scene = runtime.latest();
            }
            let frame = self.draw_surface(index, start.timing, scene.as_deref())?;
            let generation = scene.as_ref().map(|scene| scene.generation);
            if self.report_frame(index, frame, start, generation) == NextRender::Immediately {
                next = NextRender::Immediately;
            }
        }
        actions.retain(|action| {
            (self.surfaces.iter())
                .any(|surface| surface.id == action.surface && surface.lifecycle.is_open())
        });
        self.config.view_actions.restore(actions);
        // View actions queued during composition run in another frame. A configured surface that waits for no
        // frame callback can draw that frame at once.
        if self.config.view_actions.pending()
            && (self.surfaces.iter()).any(|surface| {
                surface.lifecycle.is_configured() && surface.frame_callback.is_none()
            })
        {
            next = NextRender::Immediately;
        }
        self.connection.flush().map_err(Error::Flush)?;
        Ok(next)
    }

    fn wants_frame(&self, index: usize, actions: &[PendingAction]) -> Wanted {
        let surface = &self.surfaces[index];
        let continuous =
            self.config.tick.is_some() || self.config.on_frame.is_some() || self.scene.is_some();
        if !surface.lifecycle.is_configured() || surface.frame_callback.is_some() {
            Wanted::Idle
        } else if continuous
            || surface.needs_visual_frame()
            || actions.iter().any(|action| action.surface == surface.id)
        {
            Wanted::View
        } else if surface.moving(self.start.elapsed()) {
            Wanted::Scene
        } else {
            Wanted::Idle
        }
    }

    /// Advances the scene source in the viewport of the first configured surface. Calls the tick callback of the
    /// app when at least one surface draws. Returns the latest completed scene.
    fn begin_frame(
        &mut self,
        actions: &[PendingAction],
    ) -> Result<Option<Arc<CompletedScene<SceneImage>>>, Error> {
        let frames = self.scene_frames_in_flight();
        if let Some(scene) = &mut self.scene
            && let Some(surface) =
                (self.surfaces.iter()).find(|candidate| candidate.lifecycle.is_configured())
        {
            scene.tick(
                &mut self.gpu,
                self.start.elapsed(),
                surface.logical_viewport(),
                frames,
            )?;
        }
        if (0..self.surfaces.len()).any(|index| self.wants_frame(index, actions) != Wanted::Idle) {
            let now = self.start.elapsed();
            if let Some(tick) = self.config.tick.as_mut() {
                tick(now.saturating_sub(self.last_tick));
            }
            self.last_tick = now;
        }
        Ok(self.scene.as_ref().and_then(SceneRuntime::latest))
    }

    /// The smallest number of frames in flight among the swapchains of the configured surfaces, which show the
    /// scene.
    fn scene_frames_in_flight(&self) -> Option<usize> {
        (self.surfaces.iter())
            .filter(|surface| surface.lifecycle.is_configured())
            .filter_map(|surface| surface.target.as_ref()?.frames_in_flight())
            .min()
    }

    fn composition_order(&self) -> Vec<usize> {
        let mut order: Vec<_> = (self.surfaces.iter().enumerate())
            .map(|(index, surface)| match surface.lifecycle.role() {
                Some(Role::Layer(layer)) => (layer.composition_order, index),
                _ => (0, index),
            })
            .collect();
        order.sort_unstable();
        order.into_iter().map(|(_, index)| index).collect()
    }

    /// Composes the view of surface `index` with its queued actions, and updates the role, the
    /// shape and the blur region of the surface for the frame.
    fn prepare_surface(
        &mut self,
        index: usize,
        wanted: Wanted,
        actions: &mut Vec<PendingAction>,
        frame_notified: &mut bool,
    ) -> Result<FrameStart, Error> {
        let surface = &mut self.surfaces[index];
        let viewport = surface.logical_viewport();
        let Viewport { size, density } = viewport;
        let start = FrameStart {
            wall: Instant::now(),
            cpu: self
                .config
                .observer
                .as_ref()
                .and_then(|_| thread_cpu_time()),
            timing: surface.clock.sample(self.start.elapsed()),
        };
        if !*frame_notified {
            if let Some(callback) = self.config.on_frame.as_mut() {
                callback(start.timing);
            }
            *frame_notified = true;
        }
        let mut changed = false;
        if wanted == Wanted::View {
            let mut frame = |view: &mut View, tree: &mut SceneTree| {
                view.show_frame_at(tree, viewport, &mut self.paint, start.timing)
            };
            changed = surface.view.needs_visual_frame();
            surface.show(&mut frame)?;
            let id = surface.id;
            let (own, others): (Vec<_>, Vec<_>) = std::mem::take(actions)
                .into_iter()
                .partition(|action| action.surface == id);
            *actions = others;
            for action in own {
                (action.apply)(&mut surface.view);
            }
            if surface.view.needs_visual_frame() {
                changed = true;
                surface.show(&mut frame)?;
            }
        }
        // The composed view can change the size of a layer that fits its content, or the layer layout can
        // change its margins. The layer-shell requests for that are queued now, so that the compositor
        // applies them with the buffer of this frame.
        if let Some(Role::Layer(layer)) = surface.lifecycle.role_mut() {
            if changed && let Some(proposal) = layer.content_proposal() {
                let content = (surface.view.size_that_fits(proposal, &mut self.paint))
                    .map_err(Error::MeasureView)?;
                layer.content_measured(content);
            }
            layer.update();
        }
        if let Some(bridge) = &mut surface.accessibility {
            bridge.publish(surface.view.accessibility(), false);
        }
        let shape_changed = match &mut surface.shape {
            Some(shape) => shape.update(size, density, &surface.wl, &self.compositor)?,
            None => ShapeUpdate::Unchanged,
        } == ShapeUpdate::Changed;
        if shape_changed {
            surface.damage.invalidate();
        }
        if let Some(wp_viewport) = &surface.wp_viewport {
            let (width, height) = protocol_size(surface.size)?;
            wp_viewport.set_destination(width, height);
        } else {
            surface.wl.set_buffer_scale(buffer_scale(density)?);
        }
        if let Some(background) = &surface.background
            && (shape_changed || surface.blur_size != Some(size))
        {
            let region = if let Some(shape) = &surface.shape {
                shape.region(&self.compositor)?
            } else {
                let region =
                    sctk::compositor::Region::new(&self.compositor).map_err(Error::CreateRegion)?;
                let (width, height) = protocol_size(surface.size)?;
                region.add(0, 0, width, height);
                region
            };
            background.set_blur_region(Some(region.wl_region()));
            surface.blur_size = Some(size);
        }
        Ok(start)
    }

    /// Plans the effects of surface `index`, then draws and submits its frame together with the completed
    /// `scene`.
    fn draw_surface(
        &mut self,
        index: usize,
        timing: FrameTime,
        scene: Option<&CompletedScene<SceneImage>>,
    ) -> Result<FrameOutcome, Error> {
        let surface = &mut self.surfaces[index];
        let physical = surface.physical_size()?;
        let Viewport { size, density } = surface.logical_viewport();
        let shown = surface.tree.at(timing.presentation);
        let scene_generation = scene.map_or(0, |scene| scene.generation);
        let quality = self.paint.plan_scene_effects(
            shown,
            lavashell_effect_quality::FrameBudget {
                surface: surface.id,
                gpu_time: self.config.ui_budget,
            },
            timing.sequence,
            scene_generation,
            density,
        );
        let target = (surface.target.as_mut()).ok_or(Error::SurfaceWithoutTarget)?;
        if surface.scene_generation != scene_generation
            || quality == QualityChange::Changed
            || target
                .contents(physical)
                .map_err(Error::ReadSurfaceContents)?
                == SurfaceContents::Outdated
        {
            surface.damage.invalidate();
            surface.scene_generation = scene_generation;
        }
        let whole = Rect::new(Dp::ZERO, Dp::ZERO, size.width, size.height);
        let damage = surface.damage.damage(shown, Viewport::new(size, density));
        let damage = if self.config.full_redraw {
            Some(whole)
        } else {
            damage
        };
        self.paint.set_frame_sequence(timing.sequence);
        let paint = &mut self.paint;
        let mut painted = Ok(());
        // The frame callback is requested inside the draw closure, so the Vulkan present commits it with this
        // frame. A request made for a frame that is not shown is reused from `frame_request` by the next draw.
        let frame =
            self.gpu
                .draw_timed_in(surface.id, target, physical, timing.sequence, |canvas| {
                    if surface.frame_request.is_none() {
                        let data = FrameCallbackData(surface.wl.clone());
                        surface.frame_request = Some(surface.wl.frame(&self.qh, data));
                    }
                    if let Some(presentation) = &self.presentation {
                        let feedback = presentation::Feedback {
                            surface: surface.id,
                            timing,
                        };
                        let proxy = presentation.proxy.feedback(&surface.wl, &self.qh, feedback);
                        surface.feedback.push(proxy);
                    }
                    painted = paint.paint_surface(
                        canvas,
                        lavashell_renderer::SurfaceFrame {
                            tree: shown,
                            density,
                            damage,
                            clear: lavashell_renderer::skia::Color::TRANSPARENT,
                            scene: scene.map(|scene| &scene.content),
                            shape: surface.shape.as_ref().and_then(|shape| shape.path()),
                        },
                    );
                });
        let frame = frame.map_err(Error::Draw)?;
        painted.map_err(Error::Paint)?;
        if let Some(Role::Layer(layer)) = surface.lifecycle.role_mut() {
            let input = (surface.shape.as_ref()).is_none_or(SurfaceShape::accepts_input);
            layer.presented(
                frame,
                layer::Content {
                    size: surface.size,
                    input,
                },
            );
        }
        Ok(frame)
    }

    /// Reports the frame of surface `index` to the observers of the app. Returns whether the loop renders again
    /// at once. A stale swapchain is replaced by the next draw, so that draw follows at once. A pending frame
    /// waits for a configure or a buffer release sent by the compositor. A shown frame waits for its frame
    /// callback.
    fn report_frame(
        &mut self,
        index: usize,
        frame: FrameOutcome,
        start: FrameStart,
        scene: Option<u64>,
    ) -> NextRender {
        let surface = &mut self.surfaces[index];
        if let Some(observer) = &mut self.config.view_observer {
            observer(surface.id, surface.logical_viewport(), &surface.view);
        }
        let output = surface.target.as_ref().and_then(SurfaceTarget::output);
        if let Some(info) = output
            && surface.output != Some(info)
        {
            surface.output = Some(info);
            if let Some(observer) = &mut self.config.output_observer {
                observer(index, info);
            }
        }
        if frame != FrameOutcome::Shown {
            surface.view.invalidate();
            surface.damage.invalidate();
            return if frame == FrameOutcome::Stale {
                NextRender::Immediately
            } else {
                NextRender::AfterEvent
            };
        }
        surface.view.record_presentation(start.timing.presentation);
        surface.frame_callback = surface.frame_request.take();
        let render_cost = start.wall.elapsed();
        surface.clock.render_cost(render_cost);
        if let Some(observer) = &mut self.config.observer
            && let Some(started) = start.cpu
            && let Some(now) = thread_cpu_time()
        {
            observer(FrameReport {
                surface: surface.id,
                timing: start.timing,
                stage: FrameStage::Submitted,
                observed: self.start.elapsed(),
                presentation_time: None,
                metrics: None,
                flags: None,
                submission: Some(Submission {
                    render_cost,
                    cpu_time: now.saturating_sub(started),
                    view: surface.view.stats(),
                    render: self.paint.render_stats(),
                    scene,
                    gpu: self.paint.take_gpu_frame_time(surface.id),
                }),
            });
        }
        NextRender::AfterEvent
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Wanted {
    View,
    /// The view is unchanged, but motions in the scene tree of the surface still play.
    Scene,
    Idle,
}

/// The whole number of buffer pixels along one logical pixel at `density`. A surface without a viewport sets it
/// with `wl_surface.set_buffer_scale`, which accepts only whole numbers.
fn buffer_scale(density: Density) -> Result<i32, Error> {
    let unit = crate::logical::size((1, 1));
    let (scale, _) = unit.buffer_size(density).map_err(Error::BufferSize)?;
    i32::try_from(scale).map_err(Error::BufferScale)
}

/// The width and the height of a surface as the `i32` integers used by the Wayland protocol.
fn protocol_size((width, height): (u32, u32)) -> Result<(i32, i32), Error> {
    let whole = |length| i32::try_from(length).map_err(Error::SurfaceSize);
    Ok((whole(width)?, whole(height)?))
}

/// The CPU time of the calling thread, or `None` when the clock reports a negative time.
fn thread_cpu_time() -> Option<Duration> {
    let stamp = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
    Duration::try_from(stamp).ok()
}
