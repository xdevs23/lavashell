// SPDX-License-Identifier: GPL-3.0-only

use super::painter_recipe::PainterRecipe;
use super::skia_handoff::{
    SkiaContext, SkiaDevice, SkiaFormat, SkiaImage, YcbcrConversion, backend_semaphore,
    context_state, direct_context, image_info, skia_layout,
};
use super::{cache_budget, mask_coverage};
use crate::Error;
use ash::vk;
use lavashell_core::prelude::PixelSize;
use lavashell_vulkan::{Device, Semaphore, Submission};
use skia_safe::gpu::{self, SurfaceOrigin, vk as skvk};
use skia_safe::{Canvas, Surface};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct BorrowedImage {
    pub image: vk::Image,
    pub size: PixelSize,
    pub format: vk::Format,
    pub usage: vk::ImageUsageFlags,
    pub layout: vk::ImageLayout,
    pub return_layout: vk::ImageLayout,
    pub ready: ReadySignal,
}

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

struct CheckedImage {
    format: SkiaFormat,
    size: (i32, i32),
}

impl BorrowedImage {
    pub fn check(&self) -> Result<(), Error> {
        let _drawable_image = self.checked()?;
        Ok(())
    }

    fn checked(&self) -> Result<CheckedImage, Error> {
        if self.image == vk::Image::null() {
            return Err(Error::EmbeddedImageHandle);
        }
        if !self.usage.contains(DRAWN_USAGE) {
            return Err(Error::EmbeddedImageUsage(self.usage));
        }
        let format = SkiaFormat::of(self.format).ok_or(Error::EmbeddedFormat(self.format))?;
        let size = self
            .skia_size()
            .ok_or(Error::EmbeddedImageSize(self.size))?;
        Ok(CheckedImage { format, size })
    }

    fn skia_size(&self) -> Option<(i32, i32)> {
        let (width, height) = self.size.signed()?;
        (width > 0 && height > 0).then_some((width, height))
    }
}

pub struct EmbeddedVulkanRenderer {
    context: SkiaContext,
    consumed: Option<Submission>,
    ready: Arc<Semaphore>,
    painter_recipe: PainterRecipe,
    pub device_name: String,
}

impl EmbeddedVulkanRenderer {
    /// # Safety
    /// No call of this renderer, including its creation and its drop, may overlap with a use of the queue of `device`
    /// that bypasses `device`, because Skia submits to that queue directly. No other thread may submit to the queue at
    /// the same time, and the queue must support graphics.
    pub unsafe fn new(device: &Arc<Device>) -> Result<Self, Error> {
        let api_version = device.instance().api_version();
        if api_version < vk::API_VERSION_1_1 {
            return Err(Error::HostApiVersion(api_version));
        }
        let physical = device.physical_device();
        let device_name = physical.name().map_err(Error::PhysicalDevice)?;
        let cache_bytes = cache_budget::initial_limit(&physical)?;
        let handles = SkiaDevice::of(device, YcbcrConversion::Off)?;
        let (context, glyphs) = mask_coverage::checked_context(|glyphs| {
            // SAFETY: skia-safe has no safe constructor, because Skia stores raw handles that no Rust lifetime can
            // bind. The objects of the device outlive the context, because `SkiaContext` drops the context before its
            // `Arc` of the device. By the contract of this function, the host does not use the queue while the
            // renderer uses it.
            unsafe { direct_context(&handles, glyphs, cache_bytes) }
                .map(|context| SkiaContext::of_host(context, device.clone()))
        })?;
        let ready = Semaphore::new(device).map_err(Error::ReadySemaphore)?;
        let painter_recipe = PainterRecipe::new(&physical, glyphs);
        Ok(Self {
            context,
            consumed: None,
            ready,
            painter_recipe,
            device_name,
        })
    }

    pub fn painter(&self, fonts: &crate::Fonts) -> Result<crate::Painter, crate::Error> {
        self.painter_recipe.painter(fonts)
    }

    #[must_use]
    pub fn ready_semaphore(&self) -> vk::Semaphore {
        self.ready.handle()
    }

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
            self.prepare(target)
                .map_err(|failure| match self.consume_ready(target.ready) {
                    Ok(()) => failure,
                    Err(stuck) => Error::Several(vec![failure, stuck]),
                })?;
        draw(surface.canvas());
        let state = skvk::mutable_texture_states::new_vulkan(
            return_layout,
            self.context.device().queue_family(),
        );
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

    fn prepare(
        &mut self,
        target: BorrowedImage,
    ) -> Result<(skvk::ImageLayout, Surface, gpu::BackendRenderTarget), Error> {
        cache_budget::refresh(&mut self.context)?;
        let return_layout =
            skia_layout(target.return_layout).ok_or(Error::EmbeddedLayout(target.return_layout))?;
        let (surface, backend) = self.waiting_surface(target)?;
        Ok((return_layout, surface, backend))
    }

    fn waiting_surface(
        &mut self,
        target: BorrowedImage,
    ) -> Result<(Surface, gpu::BackendRenderTarget), Error> {
        let CheckedImage { format, size } = target.checked()?;
        let layout = skia_layout(target.layout).ok_or(Error::EmbeddedLayout(target.layout))?;
        let image = SkiaImage {
            image: target.image,
            layout,
            format: format.skia,
            usage: target.usage,
            queue_family: self.context.device().queue_family(),
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

    fn consume_ready(&mut self, ready: ReadySignal) -> Result<(), Error> {
        if ready == ReadySignal::Unsignaled {
            return Ok(());
        }
        // SAFETY: lavashell-vulkan cannot check whether the semaphore has a pending signal, because only the host
        // signals it. By the contract of `draw`, the semaphore has a pending signal that nothing else waits on. The
        // draw failed before Skia waited on the semaphore, and the host does not use the queue during this call.
        let consumed = unsafe { self.context.device().wait_semaphore(&self.ready) }
            .map_err(Error::ConsumeReadySignal)?;
        self.consumed = Some(consumed);
        Ok(())
    }
}

impl Drop for EmbeddedVulkanRenderer {
    fn drop(&mut self) {
        if let Err(error) = (self.context.device())
            .signal()
            .and_then(|submission| submission.wait())
        {
            log::error!(
                "the embedded renderer cannot wait for its work on the host queue: {error}"
            );
        }
    }
}
