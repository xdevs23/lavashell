// SPDX-License-Identifier: GPL-3.0-only
mod buffer_format;
mod cache_budget;
mod completion;
mod deferred_release;
mod driver;
mod embedded;
mod external;
mod foreign;
mod mask_coverage;
mod offscreen;
mod painter_recipe;
mod presentation;
mod scene;
mod scene_mailbox;
mod skia_handoff;
mod timestamps;
mod wsi;

pub use buffer_format::ForeignFormat;
pub use completion::Completion;
pub use embedded::{BorrowedImage, EmbeddedVulkanRenderer, ReadySignal};
pub use external::{CompletedRelease, ExternalImage, PendingRelease, ReleaseProgress, SceneLease};
pub use foreign::{
    FOREIGN_IMPORT_EXTENSIONS, ForeignAlpha, ForeignImport, ForeignImporter, ForeignMemory,
    ForeignMemoryDescription, SampledMemory, Sampling,
};
pub use lavashell_vulkan::{ImageDescription, OpaqueFdExport, Status, Window};
pub use offscreen::OffscreenSurface;
pub use presentation::{HeadlessPresentation, PresentationBackend};
pub use scene::{PendingScene, SceneCompletion, SceneImage, SceneSource};
pub use scene_mailbox::{CompletedScene, SceneMailbox};
pub(crate) use skia_handoff::SkiaFormat;
pub use timestamps::GpuProfiler;
pub(crate) use timestamps::{Collected, GpuSample, SampleKind, SampleTag, SampleToken};
pub(crate) use wsi::DrawnFrame;
pub use wsi::{FrameOutcome, SurfaceAlpha, SurfaceContents, SurfaceTarget};

#[cfg(test)]
mod tests;

use crate::Error;
use ash::vk;
use lavashell_core::prelude::PixelSize;
use lavashell_vulkan::{Device, DeviceRequest, ExtensionSet, Features, Instance, InstanceRequest};
use painter_recipe::PainterRecipe;
use skia_handoff::{SkiaContext, gpu_render_target};
use skia_safe::Canvas;
use skia_safe::gpu::{self, SyncCpu};
use std::sync::Arc;
use std::time::Duration;

pub struct VulkanRenderer {
    context: SkiaContext,
    pub device_name: String,
    painter_recipe: PainterRecipe,
    imported: external::Imports,
    transform: Option<vk::SurfaceTransformFlagsKHR>,
}

impl VulkanRenderer {
    pub fn headless() -> Result<Self, Error> {
        Self::new(&HeadlessPresentation::without_surfaces())
    }

    pub fn new(backend: &impl PresentationBackend) -> Result<Self, Error> {
        let available = Instance::available_extensions().map_err(Error::InstanceExtensions)?;
        let extended = available.contains(ash::ext::swapchain_colorspace::NAME);
        let extensions = presentation::instance_extensions(backend.surface_extension(), extended);
        let request = InstanceRequest {
            api_version: vk::API_VERSION_1_1,
            extensions: &extensions,
        };
        let instance = Instance::create(&request).map_err(Error::Instance)?;
        let device = create_device(&instance, backend)?;
        let physical = device.physical_device();
        let device_name = physical.name().map_err(Error::PhysicalDevice)?;
        let cache_bytes = cache_budget::initial_limit(&physical)?;
        let (context, glyphs) = mask_coverage::checked_context(|glyphs| {
            SkiaContext::for_device(&device, glyphs, cache_bytes)
        })?;
        let painter_recipe = PainterRecipe::new(&physical, glyphs);
        Ok(Self {
            context,
            device_name,
            painter_recipe,
            imported: external::Imports::default(),
            transform: backend.transform(),
        })
    }

    pub fn surface_target(
        &self,
        window: Arc<dyn Window>,
        alpha: SurfaceAlpha,
    ) -> Result<SurfaceTarget, Error> {
        SurfaceTarget::new(self.context.device().clone(), window, alpha, self.transform)
    }

    pub fn painter(&self, fonts: &crate::Fonts) -> Result<crate::Painter, crate::Error> {
        self.painter_recipe.painter(fonts)
    }

    pub fn create_profiler(&self) -> Result<GpuProfiler, Error> {
        GpuProfiler::new(self.context.device())
    }

    pub fn offscreen(&mut self, size: PixelSize) -> Result<OffscreenSurface, Error> {
        let signed = size.signed().ok_or(Error::OffscreenSize(size))?;
        let info = crate::color::working_image_info(signed)?;
        let surface = gpu_render_target(&mut self.context, &info, gpu::Budgeted::Yes)
            .ok_or(Error::OffscreenSurface(size))?;
        Ok(OffscreenSurface::new(
            surface,
            self.context.device().clone(),
        ))
    }

    pub fn png(&mut self, surface: &mut OffscreenSurface) -> Result<Vec<u8>, Error> {
        if !surface.belongs_to(self.context.device()) {
            return Err(Error::ForeignTarget);
        }
        let context = &mut *self.context;
        context.flush_and_submit_surface(surface, Some(SyncCpu::Yes));
        let info = crate::image::png_export_info(surface.image_info().dimensions());
        let mut export =
            gpu_render_target(context, &info, gpu::Budgeted::Yes).ok_or(Error::PngSurface)?;
        let _canvas_returned_for_chaining = export.canvas().clear(skia_safe::Color::TRANSPARENT);
        let _canvas_returned_for_chaining =
            export
                .canvas()
                .draw_image(surface.image_snapshot(), (0, 0), None);
        context.flush_and_submit_surface(&mut export, Some(SyncCpu::Yes));
        let image = export.image_snapshot();
        let data = image
            .encode(context, skia_safe::EncodedImageFormat::PNG, None)
            .ok_or(Error::EncodePng)?;
        Ok(data.as_bytes().to_vec())
    }

    #[must_use]
    pub fn supports_presentation_time(&self) -> bool {
        self.context
            .device()
            .contains(ash::google::display_timing::NAME)
    }

    pub(crate) fn draw_timed_in<T>(
        &mut self,
        target: &mut SurfaceTarget,
        size: PixelSize,
        time: Option<Duration>,
        draw: impl FnOnce(&Canvas) -> T,
    ) -> Result<DrawnFrame<T>, Error> {
        let time = (time.map(|time| self.presentation_time(time))).transpose()?;
        self.draw_frame(target, size, time, draw)
    }

    fn presentation_time(&self, time: Duration) -> Result<wsi::PresentationTime, Error> {
        if !self.supports_presentation_time() {
            return Err(Error::NoDisplayTiming);
        }
        wsi::PresentationTime::new(time)
    }

    fn draw_frame<T>(
        &mut self,
        target: &mut SurfaceTarget,
        size: PixelSize,
        presentation_time: Option<wsi::PresentationTime>,
        draw: impl FnOnce(&Canvas) -> T,
    ) -> Result<DrawnFrame<T>, Error> {
        if !target.belongs_to(self.context.device()) {
            return Err(Error::ForeignTarget);
        }
        cache_budget::refresh(&mut self.context)?;
        target.draw(&mut self.context, size, presentation_time, draw)
    }
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        if let Err(error) = self.poll_scene_imports() {
            log::error!("the renderer is dropped with scene imports it cannot release: {error}");
        }
        if let Err(error) = self.context.device().wait_idle() {
            log::error!("the renderer is dropped without finishing its GPU work: {error}");
        }
        let _context_returned_for_chaining = self.context.release_resources_and_abandon();
        self.imported.clear();
    }
}

fn create_device(
    instance: &Arc<Instance>,
    backend: &impl PresentationBackend,
) -> Result<Arc<Device>, Error> {
    let picked = driver::pick_device(instance, backend)?;
    let extensions = std::iter::once(ash::khr::external_memory_fd::NAME)
        .chain(presentation::device_extensions(backend))
        .filter(|name| picked.extensions.contains(name))
        .collect::<Vec<_>>();
    let wanted = backend.features();
    let offered = (picked.physical.features()).map_err(Error::PhysicalDevice)?;
    let request = DeviceRequest {
        queue_family: picked.family,
        extensions: &extensions,
        features: Features {
            sampler_ycbcr_conversion: wanted.sampler_ycbcr_conversion
                && offered.sampler_ycbcr_conversion,
            swapchain_maintenance1: backend.surface_extension().is_some(),
        },
    };
    Device::create(picked.physical, &request).map_err(Error::Device)
}
