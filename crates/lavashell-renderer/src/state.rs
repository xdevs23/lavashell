// SPDX-License-Identifier: GPL-3.0-only
mod control;
mod error;
mod present;
mod scene_source;
mod snapshot;
mod targets;
#[cfg(test)]
mod tests;

pub use control::{PresentFrame, SurfaceControl};
pub use error::{AttachError, CommitError, DrawError, FrameError, StartError, StateError};
pub use snapshot::Snapshot;
pub use targets::DamageMode;

use crate::gpu_timing::FrameTiming;
use crate::stream::{NoStreams, StreamBackend};
use crate::text::{Shaper, TextRendering};
use crate::vulkan::{PresentationBackend, SceneSource, VulkanRenderer};
use crate::{Fonts, FrameOutcome, Painter, SurfaceAlpha, SurfaceContents};
use lavashell_core::frame::{GpuFrameTime, RenderStats};
use lavashell_core::prelude::{
    HdrMetadata, OutputInfo, OutputRequest, PixelScale, PixelSize, Rect, SceneSnapshot, Viewport,
};
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_effect_quality::FrameBudget;
use lavashell_vulkan::Window;
use present::FrameInputs;
use scene_source::SceneImages;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::sync::Arc;
use std::time::Duration;
use targets::{AttachedSurface, PixelInputs};

pub type PainterSetup = Box<dyn FnOnce(&mut Painter) + Send>;

pub type SceneSetup = Box<dyn FnOnce() -> Box<dyn SceneSource> + Send>;

pub struct RendererSetup {
    pub fonts: Fonts,
    pub painter: Option<PainterSetup>,
    pub scene: Option<SceneSetup>,
    pub frame_timing: FrameTiming,
    pub damage: DamageMode,
}

impl RendererSetup {
    #[must_use]
    pub fn shows_scene(&self) -> bool {
        self.scene.is_some()
    }
}

pub struct Attach<C> {
    pub surface: SurfaceId,
    pub window: Arc<dyn Window>,
    pub alpha: SurfaceAlpha,
    pub output: OutputRequest,
    pub hdr_metadata: Option<HdrMetadata>,
    pub control: C,
}

#[derive(Clone, Copy, Debug)]
pub struct Drawn {
    pub outcome: FrameOutcome,
    pub stats: RenderStats,
    pub gpu: Option<GpuFrameTime>,
    pub output: Option<OutputInfo>,
    pub scene_generation: Option<u64>,
}

pub struct RendererState<C: SurfaceControl, S = NoStreams> {
    painter: Painter,
    surfaces: BTreeMap<SurfaceId, AttachedSurface<C>>,
    scene: Option<SceneImages>,
    gpu: VulkanRenderer,
    streams: S,
    damage: DamageMode,
}

impl<C: SurfaceControl> RendererState<C> {
    pub fn new(
        backend: &impl PresentationBackend,
        setup: RendererSetup,
    ) -> Result<Self, StateError> {
        Self::with_streams(backend, setup, |_| Ok(NoStreams::default()))
            .map_err(StartError::into_state)
    }
}

impl<C: SurfaceControl, S: StreamBackend<Gpu = VulkanRenderer>> RendererState<C, S> {
    pub fn with_streams(
        backend: &impl PresentationBackend,
        setup: RendererSetup,
        streams: impl FnOnce(&VulkanRenderer) -> Result<S, S::Error>,
    ) -> Result<Self, StartError<S::Error>> {
        let RendererSetup {
            fonts,
            painter: configure,
            scene,
            frame_timing,
            damage,
        } = setup;
        let gpu = VulkanRenderer::new(backend).map_err(StateError::CreateRenderer)?;
        let mut painter = gpu.painter(&fonts).map_err(StateError::CreatePainter)?;
        if let Some(configure) = configure {
            configure(&mut painter);
        }
        if frame_timing == FrameTiming::Measured || painter.uses_quality_chooser() {
            let profiler = gpu.create_profiler().map_err(StateError::CreateProfiler)?;
            painter.time_gpu(profiler, frame_timing);
        }
        let streams = streams(&gpu).map_err(StartError::Streams)?;
        Ok(Self {
            painter,
            surfaces: BTreeMap::new(),
            scene: scene.map(|make| SceneImages::new(make())),
            gpu,
            streams,
            damage,
        })
    }

    pub fn attach(&mut self, attach: Attach<C>) -> Result<(), AttachError<C>> {
        let Attach {
            surface,
            window,
            alpha,
            output,
            hdr_metadata,
            control,
        } = attach;
        let Entry::Vacant(entry) = self.surfaces.entry(surface) else {
            let error = StateError::AttachedTwice(surface);
            return Err(AttachError { control, error });
        };
        let created = (self.gpu.surface_target(window, alpha))
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
                let _attached = entry.insert(AttachedSurface::new(target, control));
                Ok(())
            }
            Err(error) => Err(AttachError { control, error }),
        }
    }

    pub fn retire(&mut self, surface: SurfaceId) -> Option<C> {
        self.painter.forget_surface(surface);
        self.surfaces
            .remove(&surface)
            .map(|attached| attached.control)
    }

    #[must_use]
    pub fn control(&self, surface: SurfaceId) -> Option<&C> {
        self.surfaces
            .get(&surface)
            .map(|attached| &attached.control)
    }

    pub fn set_outline(
        &mut self,
        surface: SurfaceId,
        outline: Option<skia_safe::Path>,
    ) -> Result<(), StateError> {
        attached_mut(&mut self.surfaces, surface)?.set_outline(outline);
        Ok(())
    }

    pub fn request_output(
        &mut self,
        surface: SurfaceId,
        request: OutputRequest,
    ) -> Result<(), StateError> {
        (attached_mut(&mut self.surfaces, surface)?.target).request_output(request);
        Ok(())
    }

    #[must_use]
    pub fn output(&self, surface: SurfaceId) -> Option<OutputInfo> {
        (self.surfaces.get(&surface)).and_then(|attached| attached.target.output())
    }

    pub fn contents(
        &self,
        surface: SurfaceId,
        viewport: Viewport,
    ) -> Result<SurfaceContents, StateError> {
        let target = &attached(&self.surfaces, surface)?.target;
        target
            .contents(physical_size(viewport)?)
            .map_err(StateError::ReadContents)
    }

    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.gpu.device_name
    }

    #[must_use]
    pub fn supports_presentation_time(&self) -> bool {
        self.gpu.supports_presentation_time()
    }

    pub fn shaper(&mut self) -> &mut Shaper {
        self.painter.shaper()
    }

    pub fn run_on_streams<T>(&mut self, task: impl FnOnce(&mut S, &Painter) -> T) -> T {
        task(&mut self.streams, &self.painter)
    }

    #[must_use]
    pub fn text_rendering(&self) -> TextRendering {
        self.painter.text_rendering()
    }

    #[must_use]
    pub fn scene_deadline(&self) -> Option<Duration> {
        self.scene.as_ref().and_then(SceneImages::wake)
    }

    pub fn advance_scene(&mut self, viewport: Viewport, now: Duration) -> Result<(), StateError> {
        let frames_in_flight = self.scene_frames_in_flight();
        match &mut self.scene {
            Some(scene) => scene.tick(&mut self.gpu, now, viewport, frames_in_flight),
            None => Ok(()),
        }
    }

    pub fn draw(
        &mut self,
        surface: SurfaceId,
        frame: PresentFrame<'_>,
        snapshot: &SceneSnapshot,
    ) -> Result<Drawn, DrawError<C::Error, S::Error>> {
        self.advance_scene(frame.viewport, frame.time.now)?;
        let scene = self.scene.as_ref().and_then(SceneImages::latest);
        let physical = physical_size(frame.viewport)?;
        let shown = snapshot.at(frame.time.presentation);
        let scene_generation = scene.as_ref().map_or(0, |scene| scene.generation);
        let attached = attached_mut(&mut self.surfaces, surface)?;
        attached.prepare(&frame)?;
        let budget = FrameBudget {
            surface,
            gpu_time: frame.time.refresh,
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
        let damage = match self.damage {
            DamageMode::Whole => Some(Rect::from_size(frame.viewport.size)),
            DamageMode::Tracked => attached.damage(shown, frame.viewport, physical, &inputs)?,
        };
        let frame_inputs = FrameInputs {
            surface,
            frame,
            shown,
            physical,
            damage,
            scene: scene.as_deref(),
        };
        let (drawn, streams) =
            (self.streams).draw(&mut self.gpu, &mut self.painter, shown, |device| {
                attached.present(device, frame_inputs)
            });
        DrawError::combine(drawn, streams)
    }

    pub fn commit_state(
        &mut self,
        surface: SurfaceId,
        state: &SurfaceState,
    ) -> Result<(), CommitError<C::Error>> {
        let attached = attached_mut(&mut self.surfaces, surface)?;
        (attached.control.commit_state(state)).map_err(CommitError::Control)
    }

    pub fn control_mut(&mut self, surface: SurfaceId) -> Option<&mut C> {
        (self.surfaces.get_mut(&surface)).map(|attached| &mut attached.control)
    }

    fn scene_frames_in_flight(&self) -> Option<usize> {
        (self.surfaces.values())
            .filter_map(|attached| attached.target.frames_in_flight())
            .min()
    }
}

impl<C: SurfaceControl, S> Drop for RendererState<C, S> {
    fn drop(&mut self) {
        self.painter.retire_gpu_resources();
    }
}

fn attached<C>(
    surfaces: &BTreeMap<SurfaceId, AttachedSurface<C>>,
    surface: SurfaceId,
) -> Result<&AttachedSurface<C>, StateError> {
    surfaces
        .get(&surface)
        .ok_or(StateError::UnknownSurface(surface))
}

fn attached_mut<C>(
    surfaces: &mut BTreeMap<SurfaceId, AttachedSurface<C>>,
    surface: SurfaceId,
) -> Result<&mut AttachedSurface<C>, StateError> {
    surfaces
        .get_mut(&surface)
        .ok_or(StateError::UnknownSurface(surface))
}

fn physical_size(viewport: Viewport) -> Result<PixelSize, StateError> {
    (PixelScale::SQUARE.buffer_size(viewport)).map_err(StateError::BufferSize)
}
