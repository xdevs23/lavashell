// SPDX-License-Identifier: GPL-3.0-only
mod control;
mod error;
mod present;
mod scene_source;
mod targets;
#[cfg(test)]
mod tests;

pub use control::{PresentFrame, SurfaceControl};
pub use error::{AttachError, CommitError, DrawError, FrameError, StartError, StateError};

use crate::stream::{FrameImporter, FrameSource, LatchedFrames, NoStreams, StreamBackend};
use crate::text::{Shaper, TextRendering};
use crate::vulkan::{PresentationBackend, SceneSource, VulkanRenderer};
use crate::{
    Fonts, FrameOutcome, Latch, Painter, Stream, SurfaceAlpha, SurfaceContents, SurfaceTiming,
};
use lavashell_core::frame::{GpuFrameTime, RenderStats};
use lavashell_core::prelude::{HdrMetadata, OutputInfo, OutputRequest, SceneSnapshot, Viewport};
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_effect_quality::FrameBudget;
use lavashell_vulkan::Window;
use present::{FrameInputs, present_frame};
use scene_source::SceneImages;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::sync::Arc;
use std::time::{Duration, Instant};
use targets::{AttachedSurface, PixelInputs};

/// Runs on the painter once it exists.
pub type PainterSetup = Box<dyn FnOnce(&mut Painter) + Send>;

/// Makes the scene source on the thread that draws.
pub type SceneSetup = Box<dyn FnOnce() -> Box<dyn SceneSource> + Send>;

/// What the renderer state is made from.
pub struct RendererSetup {
    pub fonts: Fonts,
    /// Runs on the painter once it exists, for example to install effects or to replace the chooser of the effect
    /// quality.
    pub painter: Option<PainterSetup>,
    /// Makes the scene source whose latest completed image is drawn beneath the scene on every surface.
    pub scene: Option<SceneSetup>,
    /// Measures the GPU time of every frame. [`Drawn::gpu`] contains the measured times.
    pub gpu_timing: bool,
    /// Paints the whole surface in every frame instead of the area that changed.
    pub full_redraw: bool,
    /// The instant from which the times passed to the scene source are measured.
    pub start: Instant,
}

impl RendererSetup {
    /// Whether the renderer draws a scene source beneath the scene of every surface.
    #[must_use]
    pub fn shows_scene(&self) -> bool {
        self.scene.is_some()
    }
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

/// The GPU renderer, the painter used by it, the attached surfaces and the streams. Draws scene snapshots into those
/// surfaces, together with the frames of the streams that the snapshots draw.
pub struct RendererState<C: SurfaceControl, S = NoStreams> {
    painter: Painter,
    surfaces: BTreeMap<SurfaceId, AttachedSurface<C>>,
    scene: Option<SceneImages>,
    /// Declared after the painter, the targets and the scene images, so they drop before the device.
    gpu: VulkanRenderer,
    /// Declared after the device, so the frames of the streams drop once the GPU is idle and Skia has released their
    /// images.
    streams: S,
    start: Instant,
    full_redraw: bool,
}

impl<C: SurfaceControl> RendererState<C> {
    /// A state without streams whose device presents to surfaces created for `backend`.
    ///
    /// # Errors
    ///
    /// Fails when the Vulkan renderer, its painter or the GPU profiler cannot be created.
    pub fn new(
        backend: &impl PresentationBackend,
        setup: RendererSetup,
    ) -> Result<Self, StateError> {
        Self::with_streams(backend, setup, |_| Ok(NoStreams::default()))
            .map_err(StartError::into_state)
    }
}

impl<C: SurfaceControl, S: StreamBackend<Gpu = VulkanRenderer>> RendererState<C, S> {
    /// A state whose device presents to surfaces created for `backend`, with the streams made by `streams` on that
    /// device.
    ///
    /// # Errors
    ///
    /// Fails when the Vulkan renderer, its painter, the GPU profiler or the streams cannot be created.
    pub fn with_streams(
        backend: &impl PresentationBackend,
        setup: RendererSetup,
        streams: impl FnOnce(&VulkanRenderer) -> Result<S, S::Error>,
    ) -> Result<Self, StartError<S::Error>> {
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
        let streams = streams(&gpu).map_err(StartError::Streams)?;
        Ok(Self {
            painter,
            surfaces: BTreeMap::new(),
            scene: scene.map(|make| SceneImages::new(make())),
            gpu,
            streams,
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

    /// The control of `surface`, or `None` when `surface` is not attached.
    #[must_use]
    pub fn control(&self, surface: SurfaceId) -> Option<&C> {
        self.surfaces
            .get(&surface)
            .map(|attached| &attached.control)
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

    /// Sets `request` as the output of the next frames of `surface`.
    ///
    /// # Errors
    ///
    /// Fails when `surface` is not attached.
    pub fn request_output(
        &mut self,
        surface: SurfaceId,
        request: OutputRequest,
    ) -> Result<(), StateError> {
        let attached = self.surfaces.get_mut(&surface);
        let target = &mut attached.ok_or(StateError::UnknownSurface(surface))?.target;
        target.request_output(request);
        Ok(())
    }

    /// The output that the target of `surface` presents to, once the target has drawn. `None` when `surface` is not
    /// attached.
    #[must_use]
    pub fn output(&self, surface: SurfaceId) -> Option<OutputInfo> {
        (self.surfaces.get(&surface)).and_then(|attached| attached.target.output())
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

    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.gpu.device_name
    }

    /// Whether a frame can set the presentation time of its buffer through [`PresentFrame::present_at`].
    #[must_use]
    pub fn supports_presentation_time(&self) -> bool {
        self.gpu.supports_presentation_time()
    }

    /// The shaper that measures text for views and shapes the text painted by the painter of the state.
    pub fn shaper(&mut self) -> &mut Shaper {
        self.painter.shaper()
    }

    #[must_use]
    pub fn streams(&self) -> &S {
        &self.streams
    }

    /// The text rendering of the painter, which a shaper made by [`crate::text::Shaper::with_rendering`] follows to
    /// measure text as the painter shapes it.
    #[must_use]
    pub fn text_rendering(&self) -> TextRendering {
        self.painter.text_rendering()
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
    /// forgotten when the lineage of `snapshot` differs from the lineage of the last frame. The streams import the
    /// latched frames drawn by `snapshot` before the present and return the frames that the GPU has read after it.
    ///
    /// # Errors
    ///
    /// Fails when `surface` is not attached, when the control of the surface fails, when the scene source fails, or
    /// when the frame cannot be drawn or painted. When the streams fail, the error is `DrawError::StreamsOfShownFrame`
    /// with the drawn frame, or `DrawError::FrameAndStreams` when the frame failed too.
    pub fn draw(
        &mut self,
        surface: SurfaceId,
        frame: PresentFrame<'_>,
        snapshot: &SceneSnapshot,
    ) -> Result<Drawn, DrawError<C::Error, S::Error>> {
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
            .map_err(FrameError::Control)?;
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
        let frame_inputs = FrameInputs {
            surface,
            attached,
            frame,
            shown,
            physical,
            damage,
            scene: scene.as_deref(),
        };
        let (drawn, streams) =
            (self.streams).draw(&mut self.gpu, &mut self.painter, shown, |device| {
                present_frame(device, frame_inputs)
            });
        DrawError::combine(drawn, streams)
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

impl<C: SurfaceControl, I: FrameImporter> RendererState<C, LatchedFrames<I>> {
    /// Latches the newest frame of `stream` and releases the frame latched before, as [`LatchedFrames::latch`]
    /// describes. Compose the snapshot that draws the returned image after the latch, because a frame draws nothing
    /// for an image older than the one latched last.
    ///
    /// # Errors
    ///
    /// Fails as [`LatchedFrames::latch`] does.
    pub fn latch<F>(&mut self, stream: &mut Stream<F>) -> Result<Latch, I::Error>
    where
        F: FrameSource<Frame = I::Frame>,
        I::Error: From<F::Error>,
    {
        self.streams.latch(&self.painter, stream)
    }
}

impl<C: SurfaceControl, S> Drop for RendererState<C, S> {
    fn drop(&mut self) {
        self.painter.retire_gpu_resources();
    }
}

fn physical_size(viewport: Viewport) -> Result<(u32, u32), StateError> {
    (viewport.size.buffer_size(viewport.density)).map_err(StateError::BufferSize)
}
