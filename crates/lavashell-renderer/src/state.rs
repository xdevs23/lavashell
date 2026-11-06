// SPDX-License-Identifier: GPL-3.0-only
mod control;
mod scene_source;
mod targets;
#[cfg(test)]
mod tests;

pub use control::{PresentFrame, SurfaceControl};

use crate::text::Shaper;
use crate::vulkan::{PresentationBackend, SceneSource, VulkanRenderer};
use crate::{
    Error, Fonts, FrameOutcome, Painter, SurfaceAlpha, SurfaceContents, SurfaceFrame, SurfaceTiming,
};
use lavashell_core::frame::{GpuFrameTime, RenderStats};
use lavashell_core::prelude::{HdrMetadata, OutputInfo, OutputRequest, SceneSnapshot, Viewport};
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_core::units::NumberError;
use lavashell_effect_quality::FrameBudget;
use lavashell_vulkan::Window;
use scene_source::SceneImages;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use targets::{AttachedSurface, PixelInputs};

/// Runs on the painter once it exists.
pub type PainterSetup = Box<dyn FnOnce(&mut Painter)>;

/// What the renderer state is made from.
pub struct RendererSetup {
    pub fonts: Fonts,
    /// Runs on the painter once it exists, for example to install effects or to replace the chooser of the effect
    /// quality.
    pub painter: Option<PainterSetup>,
    /// Draws the latest completed image of the scene source beneath the scene on every surface.
    pub scene: Option<Box<dyn SceneSource>>,
    /// Measures the GPU time of every frame. [`Drawn::gpu`] contains the measured times.
    pub gpu_timing: bool,
    /// Paints the whole surface in every frame instead of the area that changed.
    pub full_redraw: bool,
    /// The instant from which the times passed to the scene source are measured.
    pub start: Instant,
}

/// A surface for the renderer state to draw into, with the control that makes the platform requests of each present.
pub struct Attach<C> {
    pub surface: SurfaceId,
    pub window: Arc<dyn Window>,
    pub alpha: SurfaceAlpha,
    pub timing: SurfaceTiming,
    pub output: OutputRequest,
    pub hdr_metadata: Option<HdrMetadata>,
    /// The GPU time per frame available to the effects of the surface, or `None` without a limit.
    pub gpu_budget: Option<Duration>,
    pub control: C,
}

/// The result of one draw of a surface.
#[derive(Clone, Copy, Debug)]
pub struct Drawn {
    pub outcome: FrameOutcome,
    pub stats: RenderStats,
    /// When GPU timing is on and the frame was shown, the GPU time measured since the last shown frame for the most
    /// recent earlier frame of the surface.
    pub gpu: Option<GpuFrameTime>,
    /// The output that the target of the surface presents to, once the target has drawn.
    pub output: Option<OutputInfo>,
    /// The generation of the scene image drawn beneath the scene.
    pub scene: Option<u64>,
}

/// A failure of the renderer state, named after the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("the Vulkan renderer could not start")]
    CreateRenderer(#[source] Error),
    #[error("the painter could not start")]
    CreatePainter(#[source] Error),
    #[error("the GPU profiler could not be created")]
    CreateProfiler(#[source] Error),
    #[error("the Vulkan target of a surface could not be created")]
    CreateTarget(#[source] Error),
    #[error("the HDR metadata of a surface was rejected")]
    SetHdrMetadata(#[source] Error),
    #[error("the surface {0:?} is attached already")]
    AttachedTwice(SurfaceId),
    #[error("the surface {0:?} is not attached")]
    UnknownSurface(SurfaceId),
    #[error("no buffer of whole pixels covers the size of a surface")]
    BufferSize(#[source] NumberError),
    #[error("the contents of a drawn surface could not be checked")]
    ReadContents(#[source] Error),
    #[error("the scene images that the GPU is done with could not be released")]
    ReleaseSceneImports(#[source] Error),
    #[error("the completion of a scene could not be read")]
    SceneCompletion(#[source] Error),
    #[error("the scene source could not produce a scene")]
    ProduceScene(#[source] Error),
    #[error("the scene mailbox did not accept a produced scene")]
    PublishScene(#[source] Error),
}

/// A surface that the renderer state could not attach, with the control handed in for it.
pub struct AttachError<C> {
    pub control: C,
    pub error: StateError,
}

impl<C> fmt::Debug for AttachError<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        (formatter.debug_struct("AttachError"))
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl<C> fmt::Display for AttachError<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a surface could not be attached")
    }
}

impl<C> std::error::Error for AttachError<C> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// A failure of the draw of a frame, named after the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum DrawError<E> {
    #[error(transparent)]
    State(#[from] StateError),
    #[error("a frame could not be drawn")]
    Draw(#[source] Error),
    #[error("a frame could not be painted")]
    Paint(#[source] Error),
    #[error("the platform could not prepare the present of a frame")]
    Control(#[source] E),
}

/// A failure of the commit of a surface state without a frame, named after the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum CommitError<E> {
    #[error(transparent)]
    State(#[from] StateError),
    #[error("the platform could not commit the state of a surface")]
    Control(#[source] E),
}

/// The GPU renderer, the painter used by it, and the attached surfaces. Draws scene snapshots into those surfaces.
pub struct RendererState<C: SurfaceControl> {
    painter: Painter,
    surfaces: BTreeMap<SurfaceId, AttachedSurface<C>>,
    scene: Option<SceneImages>,
    /// Declared after the painter, the targets and the scene images, so they drop before the device.
    gpu: VulkanRenderer,
    start: Instant,
    full_redraw: bool,
}

impl<C: SurfaceControl> RendererState<C> {
    /// A state whose device presents to surfaces created for `backend`.
    ///
    /// # Errors
    ///
    /// Fails when the Vulkan renderer, its painter or the GPU profiler cannot be created.
    pub fn new(
        backend: &impl PresentationBackend,
        setup: RendererSetup,
    ) -> Result<Self, StateError> {
        let RendererSetup {
            fonts,
            painter: configure,
            scene,
            gpu_timing,
            full_redraw,
            start,
        } = setup;
        let mut gpu = VulkanRenderer::new(backend).map_err(StateError::CreateRenderer)?;
        let mut painter = gpu.painter(&fonts).map_err(StateError::CreatePainter)?;
        if let Some(configure) = configure {
            configure(&mut painter);
        }
        if gpu_timing || painter.uses_quality_chooser() {
            let profiler = gpu.create_profiler().map_err(StateError::CreateProfiler)?;
            if gpu_timing {
                gpu.time_frames(profiler.clone());
            }
            painter.time_gpu(profiler);
        }
        Ok(Self {
            painter,
            surfaces: BTreeMap::new(),
            scene: scene.map(SceneImages::new),
            gpu,
            start,
            full_redraw,
        })
    }

    /// Creates the target of a surface and attaches the surface with its control.
    ///
    /// # Errors
    ///
    /// Fails when the surface is attached already, when its target cannot be created, or when its HDR metadata is
    /// invalid. The error then contains the control of the surface.
    pub fn attach(&mut self, attach: Attach<C>) -> Result<(), AttachError<C>> {
        let Attach {
            surface,
            window,
            alpha,
            timing,
            output,
            hdr_metadata,
            gpu_budget,
            control,
        } = attach;
        let Entry::Vacant(entry) = self.surfaces.entry(surface) else {
            let error = StateError::AttachedTwice(surface);
            return Err(AttachError { control, error });
        };
        let created = (self.gpu.surface_target(window, alpha, timing))
            .map_err(StateError::CreateTarget)
            .and_then(|mut target| {
                target.request_output(output);
                if let Some(metadata) = hdr_metadata {
                    target
                        .hdr_metadata(metadata)
                        .map_err(StateError::SetHdrMetadata)?;
                }
                Ok(target)
            });
        match created {
            Ok(target) => {
                let _attached = entry.insert(AttachedSurface::new(target, control, gpu_budget));
                Ok(())
            }
            Err(error) => Err(AttachError { control, error }),
        }
    }

    /// Drops the target of `surface` and everything the painter stored for it, and returns the control of the
    /// surface. Returns `None` when `surface` is not attached.
    pub fn retire(&mut self, surface: SurfaceId) -> Option<C> {
        self.painter.forget_effect_surface(surface);
        self.surfaces
            .remove(&surface)
            .map(|attached| attached.control)
    }

    /// Sets `outline`, in physical pixels, to clip every frame of `surface`. The next frame paints the whole surface.
    ///
    /// # Errors
    ///
    /// Fails when `surface` is not attached.
    pub fn set_outline(
        &mut self,
        surface: SurfaceId,
        outline: Option<skia_safe::Path>,
    ) -> Result<(), StateError> {
        let attached = self.surfaces.get_mut(&surface);
        attached
            .ok_or(StateError::UnknownSurface(surface))?
            .set_outline(outline);
        Ok(())
    }

    /// Whether the target of `surface` contains the last frame drawn at `viewport`.
    ///
    /// # Errors
    ///
    /// Fails when `surface` is not attached, or when Vulkan cannot report the extent of its surface.
    pub fn contents(
        &self,
        surface: SurfaceId,
        viewport: Viewport,
    ) -> Result<SurfaceContents, StateError> {
        let attached = self.surfaces.get(&surface);
        let target = &attached.ok_or(StateError::UnknownSurface(surface))?.target;
        target
            .contents(physical_size(viewport)?)
            .map_err(StateError::ReadContents)
    }

    /// The shaper that measures text for views and shapes the text painted by the painter of the state.
    pub fn shaper(&mut self) -> &mut Shaper {
        self.painter.shaper()
    }

    #[must_use]
    pub fn shows_scene(&self) -> bool {
        self.scene.is_some()
    }

    /// The instant at which the scene source produces its next scene, or `None` without a scene source or while it
    /// cannot submit another scene.
    #[must_use]
    pub fn scene_deadline(&self) -> Option<Instant> {
        let wake = self.scene.as_ref().and_then(SceneImages::wake)?;
        self.start.checked_add(wake)
    }

    /// Reads the completed scene images, and asks the scene source for the next one in `viewport` when it is due. A
    /// platform calls it before composing the views, so the scene work runs on the GPU while the views are composed.
    ///
    /// # Errors
    ///
    /// Fails when the scene source or the scene mailbox fails.
    pub fn advance_scene(&mut self, viewport: Viewport) -> Result<(), StateError> {
        let frames_in_flight = self.scene_frames_in_flight();
        match &mut self.scene {
            Some(scene) => scene.tick(
                &mut self.gpu,
                self.start.elapsed(),
                viewport,
                frames_in_flight,
            ),
            None => Ok(()),
        }
    }

    /// Draws `snapshot` into `surface` as `frame`, above the latest completed scene image. It advances the scene
    /// source first, which picks up a scene that completed since the last advance. Damage records of the surface are
    /// forgotten when the lineage of `snapshot` differs from the lineage of the last frame.
    ///
    /// # Errors
    ///
    /// Fails when `surface` is not attached, when the control of the surface fails, when the scene source fails, or
    /// when the frame cannot be drawn or painted.
    pub fn draw(
        &mut self,
        surface: SurfaceId,
        frame: PresentFrame<'_>,
        snapshot: &SceneSnapshot,
    ) -> Result<Drawn, DrawError<C::Error>> {
        self.advance_scene(frame.viewport)?;
        let scene = self.scene.as_ref().and_then(SceneImages::latest);
        let physical = physical_size(frame.viewport)?;
        let shown = snapshot.at(frame.time.presentation);
        let scene_generation = scene.as_ref().map_or(0, |scene| scene.generation);
        let attached =
            (self.surfaces.get_mut(&surface)).ok_or(StateError::UnknownSurface(surface))?;
        attached
            .control
            .prepare(&frame)
            .map_err(DrawError::Control)?;
        let budget = FrameBudget {
            surface,
            gpu_time: attached.gpu_budget,
        };
        let density = frame.viewport.density;
        let sequence = frame.time.sequence;
        let quality =
            self.painter
                .plan_scene_effects(shown, budget, sequence, scene_generation, density);
        let inputs = PixelInputs {
            lineage: snapshot.lineage(),
            scene_generation,
            quality,
        };
        let damage = attached.damage(shown, frame.viewport, physical, &inputs, self.full_redraw)?;
        self.painter.set_frame_sequence(sequence);
        let paint = &mut self.painter;
        let mut painted = Ok(());
        let paint_frame = |canvas: &skia_safe::Canvas| {
            attached.control.before_present(&frame);
            painted = paint.paint_surface(
                canvas,
                SurfaceFrame {
                    tree: shown,
                    density,
                    damage,
                    clear: skia_safe::Color::TRANSPARENT,
                    scene: scene.as_deref().map(|scene| &scene.content),
                    shape: attached.outline.as_ref(),
                },
            );
        };
        let outcome = (self.gpu)
            .draw_timed_in(
                surface,
                &mut attached.target,
                physical,
                sequence,
                paint_frame,
            )
            .map_err(DrawError::Draw)
            .and_then(|outcome| painted.map(|()| outcome).map_err(DrawError::Paint))
            .inspect_err(|_| attached.damage.invalidate())?;
        attached.control.after_present(outcome);
        attached.damage.forget_unless_shown(outcome);
        let gpu = (outcome == FrameOutcome::Shown)
            .then(|| self.painter.take_gpu_frame_time(surface))
            .flatten();
        Ok(Drawn {
            outcome,
            stats: self.painter.render_stats(),
            gpu,
            output: attached.target.output(),
            scene: scene.map(|scene| scene.generation),
        })
    }

    /// Makes the host apply `state` to `surface` at once, without a new frame. The state never applies while a frame
    /// of the surface is drawn.
    ///
    /// # Errors
    ///
    /// Fails when `surface` is not attached, or when the control of the surface fails.
    pub fn commit_state(
        &mut self,
        surface: SurfaceId,
        state: &SurfaceState,
    ) -> Result<(), CommitError<C::Error>> {
        let attached =
            (self.surfaces.get_mut(&surface)).ok_or(StateError::UnknownSurface(surface))?;
        (attached.control.commit_state(state)).map_err(CommitError::Control)
    }

    /// The control of `surface`, or `None` when `surface` is not attached.
    pub fn control_mut(&mut self, surface: SurfaceId) -> Option<&mut C> {
        (self.surfaces.get_mut(&surface)).map(|attached| &mut attached.control)
    }

    /// The smallest number of frames in flight among the swapchains of the attached surfaces, which show the scene.
    fn scene_frames_in_flight(&self) -> Option<usize> {
        (self.surfaces.values())
            .filter_map(|attached| attached.target.frames_in_flight())
            .min()
    }
}

impl<C: SurfaceControl> Drop for RendererState<C> {
    fn drop(&mut self) {
        self.painter.retire_gpu_resources();
    }
}

fn physical_size(viewport: Viewport) -> Result<(u32, u32), StateError> {
    (viewport.size.buffer_size(viewport.density)).map_err(StateError::BufferSize)
}
