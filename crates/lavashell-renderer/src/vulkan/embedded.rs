// SPDX-License-Identifier: GPL-3.0-only
//! Rendering into images that belong to a host engine, on the Vulkan device of the host.
//! This module never destroys the Vulkan instance, the device or the images of the host.

use super::skia_handoff::{
    Glyphs, SkiaContext, SkiaDevice, SkiaFormat, SkiaImage, backend_semaphore, context_state,
    direct_context, extension_names, image_info, skia_layout,
};
use super::{cache_budget, mask_coverage};
use crate::Error;
use ash::vk;
use lavashell_vulkan::{Device, Semaphore, Submission};
use skia_safe::gpu::{self, SurfaceOrigin, vk as skvk};
use skia_safe::{Canvas, Surface};
use std::sync::Arc;

/// A single-sample color image of the host, with the layout that the image has when the draw starts. The image needs
/// the color attachment usage and both transfer usages, because Skia copies internally, such as for backdrop filters.
#[derive(Clone, Copy)]
pub struct BorrowedImage {
    pub image: vk::Image,
    pub size: (u32, u32),
    pub format: vk::Format,
    pub usage: vk::ImageUsageFlags,
    pub layout: vk::ImageLayout,
    pub return_layout: vk::ImageLayout,
    pub ready: ReadySignal,
}

/// Whether the host signals the semaphore returned by `EmbeddedVulkanRenderer::ready_semaphore` in its submission
/// before a draw. A draw with `Signaled` waits on that semaphore.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadySignal {
    Signaled,
    Unsignaled,
}

const DRAWN_USAGE: vk::ImageUsageFlags = vk::ImageUsageFlags::from_raw(
    vk::ImageUsageFlags::COLOR_ATTACHMENT.as_raw()
        | vk::ImageUsageFlags::TRANSFER_SRC.as_raw()
        | vk::ImageUsageFlags::TRANSFER_DST.as_raw(),
);

impl BorrowedImage {
    /// # Errors
    /// Fails when the renderer cannot draw into the image, because of its format, size, handle or usage.
    pub fn check(&self) -> Result<(), Error> {
        self.usable()?;
        if SkiaFormat::of(self.format).is_none() {
            return Err(Error::EmbeddedFormat(self.format));
        }
        if self.skia_size().is_none() {
            return Err(Error::EmbeddedImageSize(self.size));
        }
        Ok(())
    }

    fn usable(&self) -> Result<(), Error> {
        if self.image == vk::Image::null() {
            return Err(Error::EmbeddedImageHandle);
        }
        if !self.usage.contains(DRAWN_USAGE) {
            return Err(Error::EmbeddedImageUsage(self.usage));
        }
        Ok(())
    }

    /// The size as the `i32` width and height expected by Skia, or `None` for an empty image or an image too large
    /// for Skia.
    fn skia_size(&self) -> Option<(i32, i32)> {
        let (width, height) = (
            i32::try_from(self.size.0).ok()?,
            i32::try_from(self.size.1).ok()?,
        );
        (width > 0 && height > 0).then_some((width, height))
    }
}

/// Paints into images of a host engine, between the submissions of the host on one queue.
pub struct EmbeddedVulkanRenderer {
    context: SkiaContext,
    /// The empty submission that waited on the ready semaphore for the last draw that failed before Skia waited on
    /// it. The submission is stored, because dropping it would make the failed draw wait for the GPU.
    consumed: Option<Submission>,
    ready: Semaphore,
    device: Arc<Device>,
    raster_paths: bool,
    uniform_range: crate::UniformRange,
    pub device_name: String,
}

impl EmbeddedVulkanRenderer {
    /// # Errors
    /// Fails when the instance of `device` is older than Vulkan 1.1, or when Skia cannot create a context on the
    /// device.
    ///
    /// # Safety
    /// No call of this renderer, including its creation and its drop, may overlap with a use of the queue of `device`
    /// that bypasses `device`, because Skia submits to that queue directly. No other thread may submit to the queue at
    /// the same time, and the queue must support graphics.
    pub unsafe fn new(device: Arc<Device>) -> Result<Self, Error> {
        let api_version = device.instance().api_version();
        if api_version < vk::API_VERSION_1_1 {
            return Err(Error::HostApiVersion(api_version));
        }
        let physical = device.physical_device();
        let device_name = physical.name().map_err(Error::PhysicalDevice)?;
        let cache_bytes = cache_budget::limit(&physical, 0)?;
        let uniform_range =
            crate::UniformRange::of_device(physical.properties().limits.max_uniform_buffer_range);
        let instance = device.instance();
        let handles = SkiaDevice {
            entry: instance.entry(),
            instance: instance.ash(),
            physical: physical.handle(),
            device: device.ash(),
            queue: device.queue_handle(),
            family: device.queue_family(),
            api_version: instance.api_version(),
            instance_extensions: &extension_names(instance.extensions())?,
            device_extensions: &extension_names(device.extensions())?,
            ycbcr_conversion: false,
        };
        let (context, glyphs) = mask_coverage::checked_context(|glyphs| {
            // SAFETY: the objects of the device outlive the context, because the renderer drops the context before its
            // `Arc` of the device. By the contract of this function, the host does not use the queue while the
            // renderer uses it.
            unsafe { direct_context(&handles, glyphs, cache_bytes) }.map(SkiaContext::of_host)
        })?;
        let ready = Semaphore::new(&device).map_err(Error::ReadySemaphore)?;
        Ok(Self {
            context,
            consumed: None,
            ready,
            device,
            raster_paths: glyphs == Glyphs::Paths,
            uniform_range,
            device_name,
        })
    }

    /// # Errors
    /// Fails when the painter cannot compile its shader programs or load its fonts.
    pub fn painter(&self, fonts: &crate::Fonts) -> Result<crate::Painter, crate::Error> {
        Ok(crate::Painter::new(fonts, self.uniform_range)?.with_raster_paths(self.raster_paths))
    }

    /// A binary semaphore on the host device for `ReadySignal::Signaled`. The host signals the semaphore in the
    /// submission before a draw. The semaphore exists as long as the renderer.
    #[must_use]
    pub fn ready_semaphore(&self) -> vk::Semaphore {
        self.ready.handle()
    }

    /// Paints over the existing contents of the image and submits the draw without waiting on a fence on the CPU.
    ///
    /// # Errors
    /// Fails when Skia does not support the format, the size or the layouts of the image, or when Skia cannot wrap the
    /// image or submit the draw.
    ///
    /// # Safety
    /// The image must belong to the host device given to `new`, be owned exclusively by its queue family, have one
    /// sample, one mip level and one layer, and exist until the submissions of this draw finish. Earlier host writes
    /// must be submitted on the same queue with a memory dependency that makes them visible to color attachment reads
    /// and writes. `layout` must be the layout recorded for the image by the resource tracker of the host. Later host
    /// reads must depend on these writes and start in `return_layout`.
    /// With `ReadySignal::Signaled`, the ready semaphore must have a pending signal that nothing else waits on.
    /// The queue contract of `new` applies to the whole call.
    /// When the draw fails before Skia submits, the renderer still waits on that signal in a submission of its own.
    pub unsafe fn draw(
        &mut self,
        target: BorrowedImage,
        draw: impl FnOnce(&Canvas),
    ) -> Result<(), Error> {
        let (return_layout, mut surface, backend) =
            (self.prepare(target)).inspect_err(|_| self.consume_ready(target.ready))?;
        draw(surface.canvas());
        let state =
            skvk::mutable_texture_states::new_vulkan(return_layout, self.device.queue_family());
        let context = &mut *self.context;
        context.flush_surface(&mut surface);
        let handed_back = context.set_backend_render_target_state(&backend, &state);
        let submitted = context.submit(None);
        if !handed_back {
            return Err(Error::EmbeddedReturnLayout(target.return_layout));
        }
        if !submitted {
            return Err(Error::EmbeddedSubmit);
        }
        Ok(())
    }

    /// Prepares `target` for a draw under the contract of `draw`. Returns the Skia layout in which `target` goes back
    /// to the host, a surface on `target`, and the render target wrapped by the surface. The next submission of the
    /// surface waits on the ready semaphore when the host signaled it.
    fn prepare(
        &mut self,
        target: BorrowedImage,
    ) -> Result<(skvk::ImageLayout, Surface, gpu::BackendRenderTarget), Error> {
        cache_budget::refresh(&mut self.context, &self.device)?;
        let return_layout =
            skia_layout(target.return_layout).ok_or(Error::EmbeddedLayout(target.return_layout))?;
        let (surface, backend) = self.waiting_surface(target)?;
        Ok((return_layout, surface, backend))
    }

    /// A surface on `target`, and the render target wrapped by the surface, under the contract of `draw`. The next
    /// submission of the surface waits on the ready semaphore when the host signaled it.
    fn waiting_surface(
        &mut self,
        target: BorrowedImage,
    ) -> Result<(Surface, gpu::BackendRenderTarget), Error> {
        target.usable()?;
        let format = SkiaFormat::of(target.format).ok_or(Error::EmbeddedFormat(target.format))?;
        let size = target
            .skia_size()
            .ok_or(Error::EmbeddedImageSize(target.size))?;
        let layout = skia_layout(target.layout).ok_or(Error::EmbeddedLayout(target.layout))?;
        let image = SkiaImage {
            image: target.image,
            layout,
            format: format.skia,
            usage: target.usage,
            queue_family: self.device.queue_family(),
            conversion: None,
        };
        // SAFETY: Skia receives the host image without a lifetime. By the contract of `draw`, the only caller, the
        // image exists until the submissions of the draw finish.
        let info = unsafe { image_info(&image) }?;
        let backend = gpu::backend_render_targets::make_vk(size, &info);
        let context = &mut *self.context;
        let mut surface = gpu::surfaces::wrap_backend_render_target(
            context,
            &backend,
            SurfaceOrigin::TopLeft,
            format.color_type,
            Some(format.color_space()),
            None,
        )
        .ok_or_else(|| Error::WrapHostImage {
            format: target.format,
            usage: target.usage,
            size: target.size,
            context: context_state(context),
        })?;
        if target.ready == ReadySignal::Signaled {
            // SAFETY: skia-safe accepts a semaphore only as a handle without a lifetime. The renderer owns the
            // semaphore. Dropping the renderer waits for the queue before the semaphore is destroyed, so the
            // semaphore outlives the submissions of the draw.
            let semaphore = unsafe { backend_semaphore(self.ready.handle()) }?;
            if !surface.wait(&[semaphore], false) {
                return Err(Error::WaitSemaphore);
            }
        }
        Ok((surface, backend))
    }

    /// Submits an empty batch that waits on the ready semaphore when the host signaled it. The batch consumes the
    /// signal meant for a draw that failed before Skia waited on the semaphore.
    fn consume_ready(&mut self, ready: ReadySignal) {
        if ready == ReadySignal::Unsignaled {
            return;
        }
        // SAFETY: lavashell-vulkan cannot check whether the semaphore has a pending signal, because only the host
        // signals it. By the contract of `draw`, the semaphore has a pending signal that nothing else waits on. The
        // draw failed before Skia waited on the semaphore, and the host does not use the queue during this call.
        match unsafe { self.device.wait_semaphore(&self.ready) } {
            Ok(consumed) => self.consumed = Some(consumed),
            Err(error) => {
                log::error!("the semaphore of a failed embedded draw stays signaled: {error}");
            }
        }
    }
}

impl Drop for EmbeddedVulkanRenderer {
    fn drop(&mut self) {
        // Dropping the renderer waits for the queue once, after the host has stopped producing frames. A normal frame
        // does not wait, because ordered submissions and image barriers synchronize it.
        if let Err(error) = self
            .device
            .signal()
            .and_then(|submission| submission.wait())
        {
            log::error!(
                "the embedded renderer cannot wait for its work on the host queue: {error}"
            );
        }
    }
}
