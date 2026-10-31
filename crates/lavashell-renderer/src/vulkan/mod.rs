// SPDX-License-Identifier: GPL-3.0-only
#[cfg(target_os = "android")]
mod android;
mod buffer_format;
mod cache_budget;
mod completion;
mod driver;
mod embedded;
mod external;
mod foreign;
mod mask_coverage;
mod offscreen;
mod presentation;
mod scene;
mod scene_mailbox;
mod skia_handoff;
mod surface_timing;
mod timestamps;
#[cfg(target_os = "linux")]
mod wayland;
mod wsi;

pub use buffer_format::ForeignFormat;
pub use completion::Completion;
pub use embedded::{BorrowedImage, EmbeddedVulkanRenderer, ReadySignal};
pub use external::{CompletedRelease, ExternalImage, PendingRelease, ReleaseProgress};
pub use foreign::{
    FOREIGN_IMPORT_EXTENSIONS, ForeignAlpha, ForeignImport, ForeignImporter, ForeignMemory,
    SampledMemory, Sampling,
};
pub use lavashell_vulkan::{ImageDescription, OpaqueFdExport, Status};
pub use offscreen::OffscreenSurface;
pub use scene::{PendingScene, SceneCompletion, SceneImage, SceneSource};
pub use scene_mailbox::{CompletedScene, SceneMailbox};
pub(crate) use skia_handoff::SkiaFormat;
pub use surface_timing::{PresentationPolicy, SurfaceTiming};
pub use timestamps::GpuProfiler;
pub(crate) use timestamps::{Collected, GpuSample, SampleKind, SampleTag};
#[cfg(target_os = "linux")]
pub use wayland::WaylandTarget;
pub use wsi::{FrameOutcome, SurfaceAlpha, SurfaceContents, SurfaceTarget};

#[cfg(test)]
mod tests;

use crate::Error;
use ash::vk;
use completion::drop_completed;
use lavashell_core::surface::SurfaceId;
use lavashell_vulkan::{Device, DeviceRequest, ExtensionSet, Features, Instance, InstanceRequest};
use skia_handoff::{Glyphs, SkiaContext};
use skia_safe::gpu::{self, SurfaceOrigin, SyncCpu};
use skia_safe::{Canvas, ColorType, Surface};
use std::rc::Rc;
use std::sync::Arc;

pub struct VulkanRenderer {
    context: SkiaContext,
    device: Arc<Device>,
    pub device_name: String,
    raster_paths: bool,
    uniform_range: crate::UniformRange,
    frame_profiler: Option<Rc<std::cell::RefCell<GpuProfiler>>>,
    failures: crate::failure::Failures,
    imported: Rc<std::cell::RefCell<external::Imports>>,
}

impl VulkanRenderer {
    /// A renderer that draws offscreen only, without a window system.
    ///
    /// # Errors
    /// Fails when no Vulkan device meets the requirements of the renderer, or when Vulkan or Skia cannot create the
    /// renderer.
    pub fn headless() -> Result<Self, Error> {
        Self::create(&presentation::Headless)
    }

    fn create(platform: &impl presentation::Presentation) -> Result<Self, Error> {
        let available = Instance::available_extensions().map_err(Error::InstanceExtensions)?;
        let extended = available.contains(ash::ext::swapchain_colorspace::NAME);
        let extensions = presentation::instance_extensions(platform.surface_extension(), extended);
        let request = InstanceRequest {
            api_version: vk::API_VERSION_1_1,
            extensions: &extensions,
        };
        let instance = Instance::create(&request).map_err(Error::Instance)?;
        let device = create_device(&instance, platform)?;
        let physical = device.physical_device();
        let device_name = physical.name().map_err(Error::PhysicalDevice)?;
        let cache_bytes = cache_budget::limit(&physical, 0)?;
        let uniform_range =
            crate::UniformRange::of_device(physical.properties().limits.max_uniform_buffer_range);
        let (context, glyphs) = mask_coverage::checked_context(|glyphs| {
            SkiaContext::for_device(&device, glyphs, cache_bytes)
        })?;
        Ok(Self {
            context,
            device,
            device_name,
            raster_paths: glyphs == Glyphs::Paths,
            uniform_range,
            frame_profiler: None,
            failures: crate::failure::Failures::default(),
            imported: Rc::new(std::cell::RefCell::new(external::Imports::default())),
        })
    }

    /// Creates a painter for the device of this renderer. The painter uses the uniform buffer range of the device,
    /// and draws glyphs as paths where the device corrupts the coverage of the glyph atlas or where that coverage
    /// could not be checked.
    ///
    /// # Errors
    /// Fails when the painter cannot compile its shader programs or load its fonts.
    pub fn painter(&self, fonts: &crate::Fonts) -> Result<crate::Painter, crate::Error> {
        Ok(crate::Painter::new(fonts, self.uniform_range)?.with_raster_paths(self.raster_paths))
    }

    /// # Errors
    /// Fails when the queue of the device records no timestamps, or the driver cannot create a GPU timer.
    pub fn create_profiler(&self) -> Result<Rc<std::cell::RefCell<GpuProfiler>>, Error> {
        let profiler = GpuProfiler::new(&self.device)?;
        Ok(Rc::new(std::cell::RefCell::new(profiler)))
    }

    pub fn time_frames(&mut self, profiler: Rc<std::cell::RefCell<GpuProfiler>>) {
        self.frame_profiler = Some(profiler);
    }

    /// # Errors
    /// Fails when `size` is too large for Skia, or when Skia cannot allocate a surface of that size.
    pub fn offscreen(&mut self, size: (u32, u32)) -> Result<OffscreenSurface, Error> {
        let (Ok(width), Ok(height)) = (i32::try_from(size.0), i32::try_from(size.1)) else {
            return Err(Error::OffscreenSize(size));
        };
        let info = skia_safe::ImageInfo::new(
            (width, height),
            ColorType::RGBAF16,
            skia_safe::AlphaType::Premul,
            crate::color::working_color_space().ok_or(Error::WorkingColorSpace)?,
        );
        let surface = gpu::surfaces::render_target(
            &mut self.context,
            gpu::Budgeted::Yes,
            &info,
            Some(0),
            Some(SurfaceOrigin::TopLeft),
            None,
            false,
            false,
        )
        .ok_or(Error::OffscreenSurface(size))?;
        Ok(OffscreenSurface::new(surface, self.device.clone()))
    }

    /// # Errors
    /// Fails when Skia cannot allocate, draw or encode the SDR copy of `surface`.
    pub fn png(&mut self, surface: &mut Surface) -> Result<Vec<u8>, Error> {
        let context = &mut *self.context;
        context.flush_and_submit_surface(surface, Some(SyncCpu::Yes));
        // The PNG export is SDR sRGB.
        let mut export = gpu::surfaces::render_target(
            context,
            gpu::Budgeted::Yes,
            &skia_safe::ImageInfo::new_n32_premul(
                surface.image_info().dimensions(),
                skia_safe::ColorSpace::new_srgb(),
            ),
            Some(0),
            Some(SurfaceOrigin::TopLeft),
            None,
            false,
            false,
        )
        .ok_or(Error::PngSurface)?;
        let _cleared_canvas = export.canvas().clear(skia_safe::Color::TRANSPARENT);
        let _drawn_canvas = export
            .canvas()
            .draw_image(surface.image_snapshot(), (0, 0), None);
        context.flush_and_submit_surface(&mut export, Some(SyncCpu::Yes));
        let image = export.image_snapshot();
        let data = image
            .encode(context, skia_safe::EncodedImageFormat::PNG, None)
            .ok_or(Error::EncodePng)?;
        Ok(data.as_bytes().to_vec())
    }

    /// # Errors
    /// Fails when the target belongs to another renderer, or the swapchain cannot present.
    pub fn draw(
        &mut self,
        target: &mut SurfaceTarget,
        size: (u32, u32),
        draw: impl FnOnce(&Canvas),
    ) -> Result<FrameOutcome, Error> {
        self.draw_timed_in(SurfaceId::UNSPECIFIED, target, size, 0, draw)
    }

    /// # Errors
    /// Fails as `draw` does.
    pub fn draw_timed_in(
        &mut self,
        surface: SurfaceId,
        target: &mut SurfaceTarget,
        size: (u32, u32),
        sequence: u64,
        draw: impl FnOnce(&Canvas),
    ) -> Result<FrameOutcome, Error> {
        self.draw_frame(target, size, (surface, sequence), None, draw)
    }

    fn draw_frame(
        &mut self,
        target: &mut SurfaceTarget,
        size: (u32, u32),
        sample: (SurfaceId, u64),
        presentation_time: Option<wsi::PresentationTime>,
        draw: impl FnOnce(&Canvas),
    ) -> Result<FrameOutcome, Error> {
        if !target.belongs_to(&self.device) {
            return Err(Error::ForeignTarget);
        }
        cache_budget::refresh(&mut self.context, &self.device)?;
        let profiler = self.frame_profiler.clone();
        let mut started = Ok(None);
        let result = target.draw(&mut self.context, size, presentation_time, |canvas| {
            if let Some(profiler) = &profiler {
                if let Some(mut context) = canvas.direct_context() {
                    let _chained_context = context.flush_and_submit();
                }
                started = profiler
                    .borrow_mut()
                    .begin(SampleTag::frame(sample.0, sample.1))
                    .map(Some);
            }
            draw(canvas);
        });
        if let Some(profiler) = profiler {
            let timed = started.and_then(|token| match token {
                Some(token) => profiler.borrow_mut().end(token),
                None => Ok(()),
            });
            if let Err(cause) = timed {
                (self.failures).record_caused(crate::failure::DrawFailure::GpuTimestamp, &cause);
            }
        }
        result
    }
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        if let Err(error) = self.poll_scene_imports() {
            log::error!("the renderer is dropped with scene imports it cannot release: {error}");
        }
        if let Err(error) = self.device.wait_idle() {
            log::error!("the renderer is dropped without finishing its GPU work: {error}");
        }
        let _chained_context = self.context.release_resources_and_abandon();
        self.imported.borrow_mut().clear();
    }
}

fn create_device(
    instance: &Arc<Instance>,
    platform: &impl presentation::Presentation,
) -> Result<Arc<Device>, Error> {
    let picked = driver::pick_device(instance, platform)?;
    let extensions = std::iter::once(ash::khr::external_memory_fd::NAME)
        .chain(platform.device_extensions())
        .filter(|name| picked.extensions.contains(name))
        .collect::<Vec<_>>();
    let wanted = platform.features();
    let offered = (picked.physical.features()).map_err(Error::PhysicalDevice)?;
    let request = DeviceRequest {
        queue_family: picked.family,
        extensions: &extensions,
        features: Features {
            sampler_ycbcr_conversion: wanted.sampler_ycbcr_conversion
                && offered.sampler_ycbcr_conversion,
        },
    };
    Device::create(picked.physical, &request).map_err(Error::Device)
}
