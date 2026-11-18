// SPDX-License-Identifier: GPL-3.0-only
mod error;
mod present;
mod scene_source;
mod targets;
#[cfg(test)]
mod tests;

pub use error::{CommitError, DrawError, FrameError, StartError, StateError};

use crate::vulkan::{PresentationBackend, SceneSource, VulkanRenderer};
use crate::{NoStreams, RendererStreams, VulkanWindow};
use lavashell_core::prelude::{
    OutputInfo, OutputRequest, PixelSize, Rect, ScaledViewport, SceneSnapshot,
};
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_effect_quality::FrameBudget;
use lavashell_platform::{
    Attach, Drawn, PresentFrame, RendererSetup, StreamTask, SurfaceContents, SurfaceControl,
    SurfaceRenderer,
};
use lavashell_renderer::text::{Shaper, TextRendering};
use lavashell_renderer::{DamageMode, FrameTiming, Painter, StreamBackend};
use present::FrameInputs;
use scene_source::SceneImages;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::time::Duration;
use targets::{AttachedSurface, PixelInputs};

pub type SceneSetup = Box<dyn FnOnce() -> Box<dyn SceneSource> + Send>;

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
        Self::with_streams(backend, setup, None, |_| Ok(NoStreams::default()))
            .map_err(StartError::into_state)
    }
}

impl<C: SurfaceControl, S: StreamBackend<Gpu = VulkanRenderer>> RendererState<C, S> {
    pub fn with_streams(
        backend: &impl PresentationBackend,
        setup: RendererSetup,
        scene: Option<SceneSetup>,
        streams: impl FnOnce(&VulkanRenderer) -> Result<S, S::Error>,
    ) -> Result<Self, StartError<S::Error>> {
        let RendererSetup {
            fonts,
            painter: configure,
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

    #[must_use]
    pub fn control(&self, surface: SurfaceId) -> Option<&C> {
        self.surfaces
            .get(&surface)
            .map(|attached| &attached.control)
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

    fn scene_frames_in_flight(&self) -> Option<usize> {
        (self.surfaces.values())
            .filter_map(|attached| attached.target.frames_in_flight())
            .min()
    }
}

impl<C: SurfaceControl, S: RendererStreams> SurfaceRenderer<C, VulkanWindow, S>
    for RendererState<C, S>
{
    type Error = StateError;
    type DrawError = DrawError<C::Error, S::Error>;
    type CommitError = CommitError<C::Error>;

    fn text_rendering(&self) -> TextRendering {
        self.painter.text_rendering()
    }

    fn attach(&mut self, attach: Attach<C, VulkanWindow>) -> Result<(), StateError> {
        let Attach {
            surface,
            window,
            alpha,
            output,
            hdr_metadata,
            control,
        } = attach;
        let Entry::Vacant(entry) = self.surfaces.entry(surface) else {
            return Err(StateError::AttachedTwice(surface));
        };
        let mut target =
            (self.gpu.surface_target(window, alpha)).map_err(StateError::CreateTarget)?;
        target.request_output(output);
        if let Some(metadata) = hdr_metadata {
            target
                .hdr_metadata(metadata)
                .map_err(StateError::SetHdrMetadata)?;
        }
        let _attached_surface_is_not_needed = entry.insert(AttachedSurface::new(target, control));
        Ok(())
    }

    fn retire(&mut self, surface: SurfaceId) -> Option<C> {
        self.painter.forget_surface(surface);
        self.surfaces
            .remove(&surface)
            .map(|attached| attached.control)
    }

    fn control_mut(&mut self, surface: SurfaceId) -> Option<&mut C> {
        (self.surfaces.get_mut(&surface)).map(|attached| &mut attached.control)
    }

    fn set_outline(
        &mut self,
        surface: SurfaceId,
        outline: Option<skia_safe::Path>,
    ) -> Result<(), StateError> {
        attached_mut(&mut self.surfaces, surface)?.set_outline(outline);
        Ok(())
    }

    fn run_on_streams(&mut self, task: StreamTask<S>) {
        task(&mut self.streams, &self.painter);
    }

    fn scene_deadline(&self) -> Option<Duration> {
        self.scene.as_ref().and_then(SceneImages::wake)
    }

    fn advance_scene(
        &mut self,
        scaled_viewport: ScaledViewport,
        now: Duration,
    ) -> Result<(), StateError> {
        let frames_in_flight = self.scene_frames_in_flight();
        match &mut self.scene {
            Some(scene) => scene.tick(&mut self.gpu, now, scaled_viewport, frames_in_flight),
            None => Ok(()),
        }
    }

    fn draw(
        &mut self,
        surface: SurfaceId,
        frame: PresentFrame<'_>,
        snapshot: &SceneSnapshot,
    ) -> Result<Drawn, DrawError<C::Error, S::Error>> {
        self.advance_scene(frame.scaled_viewport, frame.time.now)?;
        let scene = self.scene.as_ref().and_then(SceneImages::latest);
        let physical = physical_size(frame.scaled_viewport)?;
        let shown = snapshot.at(frame.time.presentation);
        let scene_generation = scene.as_ref().map_or(0, |scene| scene.generation);
        let attached = attached_mut(&mut self.surfaces, surface)?;
        attached.prepare(&frame)?;
        attached.damage.draw_at(frame.scaled_viewport.scale);
        let budget = FrameBudget {
            surface,
            gpu_time: frame.time.refresh,
        };
        let sequence = frame.time.sequence;
        let dp_to_output = frame.scaled_viewport.dp_to_output();
        let quality = (self.painter).plan_scene_effects(
            shown,
            budget,
            sequence,
            scene_generation,
            dp_to_output,
        );
        let inputs = PixelInputs {
            lineage: snapshot.lineage(),
            scene_generation,
            quality,
        };
        let viewport = frame.scaled_viewport.viewport;
        let damage = match self.damage {
            DamageMode::Whole => Some(Rect::from_size(viewport.size)),
            DamageMode::Tracked => attached.damage(shown, viewport, physical, &inputs)?,
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

    fn contents(
        &self,
        surface: SurfaceId,
        scaled_viewport: ScaledViewport,
    ) -> Result<SurfaceContents, StateError> {
        let attached = attached(&self.surfaces, surface)?;
        let size = physical_size(scaled_viewport)?;
        (attached.target.contents(size)).map_err(StateError::ReadContents)
    }

    fn commit_state(
        &mut self,
        surface: SurfaceId,
        state: &SurfaceState,
    ) -> Result<(), CommitError<C::Error>> {
        let attached = attached_mut(&mut self.surfaces, surface)?;
        (attached.control.commit_state(state)).map_err(CommitError::Control)
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

fn physical_size(scaled_viewport: ScaledViewport) -> Result<PixelSize, StateError> {
    (scaled_viewport.buffer_size()).map_err(StateError::BufferSize)
}
