// SPDX-License-Identifier: GPL-3.0-only
use super::extent::Extent;
use super::{Chain, FrameOutcome, PresentationTime, SurfaceTarget, present};
use crate::Error;
use crate::scalar::scalar;
use crate::vulkan::skia_handoff::{
    SkiaFormat, SkiaImage, backend_semaphore, context_state, image_info,
};
use ash::vk;
use lavashell_core::prelude::OutputProfile;
use lavashell_vulkan::{Acquisition, Fit, Presented, Semaphore};
use skia_safe::gpu::{self, DirectContext, FlushInfo, SurfaceOrigin, vk as skvk};
use skia_safe::{Canvas, ColorType, Surface};
use std::time::Duration;

/// The result of a frame that did not fail.
enum Frame {
    Shown(Fit),
    NotReady,
    OutOfDate,
}

impl SurfaceTarget {
    pub(in crate::vulkan) fn draw(
        &mut self,
        context: &mut DirectContext,
        size: (u32, u32),
        presentation_time: Option<PresentationTime>,
        draw: impl FnOnce(&Canvas),
    ) -> Result<FrameOutcome, Error> {
        let timing = self.presentation.clone();
        present::before_acquisition(&timing, size, presentation_time, || {
            self.draw_ready(context, size, presentation_time, draw)
        })
    }

    fn draw_ready(
        &mut self,
        context: &mut DirectContext,
        size: (u32, u32),
        presentation_time: Option<PresentationTime>,
        draw: impl FnOnce(&Canvas),
    ) -> Result<FrameOutcome, Error> {
        let mut chain = self.current_chain(size)?;
        let frame = self.draw_chain(&mut chain, context, presentation_time, draw);
        self.chain = Some(chain);
        match frame {
            // A target with an explicit transform goes on presenting with a chain that Vulkan reports as SUBOPTIMAL,
            // because its transform differs from the current transform of the surface on purpose. A change of the
            // window size and OUT_OF_DATE still recreate the chain.
            Ok(Frame::Shown(fit)) => {
                self.stale = fit == Fit::Suboptimal && self.transform.is_none();
                Ok(FrameOutcome::Shown)
            }
            Ok(Frame::NotReady) => Ok(FrameOutcome::Pending),
            Ok(Frame::OutOfDate) => {
                self.stale = true;
                Ok(FrameOutcome::Stale)
            }
            // A frame that fails after its acquisition leaves the semaphore of its slot signaled, so the next frame
            // replaces the chain together with its slots.
            Err(error) => {
                self.stale = true;
                Err(error)
            }
        }
    }

    /// The chain for drawing at `size`. The chain of the target is reused, or created anew when the target is stale,
    /// when the extent of the surface changed, or when the chain has another size.
    fn current_chain(&mut self, size: (u32, u32)) -> Result<Chain, Error> {
        if self.surface_extent()? == Extent::Changed {
            self.stale = true;
        }
        match self.chain.take() {
            Some(chain) if !self.stale && chain.requested_size == size => Ok(chain),
            old => {
                let chain = self.create_chain(size, old)?;
                self.stale = false;
                Ok(chain)
            }
        }
    }

    fn draw_chain(
        &self,
        chain: &mut Chain,
        context: &mut DirectContext,
        presentation_time: Option<PresentationTime>,
        draw: impl FnOnce(&Canvas),
    ) -> Result<Frame, Error> {
        let slot = chain.next_slot;
        if let Some(frame) = &chain.slots[slot].frame {
            frame.wait().map_err(Error::WaitFrame)?;
        }
        // SAFETY: the swapchain API of lavashell-vulkan has no safe acquisition, because only the caller can track the
        // state of the semaphore. The semaphore of the slot has no signal or wait pending. An acquisition that
        // returned no image signaled nothing. The last image acquired with this semaphore was waited for by the Skia
        // flush of its frame. The completion of that frame, stored in the slot, was submitted after the flush and
        // has completed. A frame that failed in between made the chain stale, so the chain and its slots were
        // replaced.
        let acquired = unsafe { chain.swapchain.acquire(&chain.slots[slot].acquire) }
            .map_err(Error::Acquire)?;
        let (index, acquired_fit) = match acquired {
            Acquisition::Image { index, fit } => (index, fit),
            Acquisition::NotReady => return Ok(Frame::NotReady),
            Acquisition::OutOfDate => return Ok(Frame::OutOfDate),
        };
        let image = index as usize;
        let extent = skia_extent(chain.swapchain.request().extent)?;
        let mut surface = self.image_surface(chain, image, extent, context)?;
        // SAFETY: skia-safe has no safe way to hand over a semaphore, because Skia stores the raw handle without a
        // lifetime. The slot owns the semaphore. The chain contains the slot until the completion of this frame is
        // observed, and that completion follows the wait.
        let acquisition = unsafe { backend_semaphore(chain.slots[slot].acquire.handle()) }?;
        if !surface.wait(&[acquisition], false) {
            return Err(Error::WaitAcquisition);
        }
        let working = working_surface(&mut chain.working, context, extent)?;
        draw(working.canvas());
        let mut output = skia_safe::Paint::default();
        let output = (output
            .set_blend_mode(skia_safe::BlendMode::Src)
            .set_dither(true))
        .set_color_filter(output_color_filter(chain.info.profile)?);
        working.draw(
            surface.canvas(),
            (0, 0),
            skia_safe::SamplingOptions::default(),
            Some(&*output),
        );
        submit(
            context,
            &mut surface,
            &chain.finished[image],
            self.device.queue_family(),
        )?;
        drop(surface);
        chain.slots[slot].frame = Some(self.device.signal().map_err(Error::SignalFrame)?);
        chain.layouts[image] = skvk::ImageLayout::PRESENT_SRC_KHR;
        chain.next_slot = (chain.next_slot + 1) % chain.slots.len();
        self.presentation.submitting(presentation_time.is_some());
        let time = presentation_time.map(|time| Duration::from_nanos(time.nanos()));
        // SAFETY: the swapchain API of lavashell-vulkan has no safe presentation, because only the caller can track the
        // acquisition and the semaphore. Image `index` was acquired above and is presented once. The flush above
        // submitted the signal of the semaphore of the image after the writes to the image. Only this presentation
        // waits on that signal. A later frame signals the semaphore again only after it acquires the image anew, and
        // the presentation engine returns the image for a new acquisition only after this wait is consumed.
        let presented = unsafe { chain.swapchain.present(index, &chain.finished[image], time) };
        Ok(match presented.map_err(Error::Present)? {
            Presented::Shown(fit) if acquired_fit == Fit::Suboptimal || fit == Fit::Suboptimal => {
                Frame::Shown(Fit::Suboptimal)
            }
            Presented::Shown(_) => Frame::Shown(Fit::Optimal),
            Presented::OutOfDate => Frame::OutOfDate,
        })
    }

    /// A Skia surface that wraps the swapchain image `image`, in the layout in which the last frame left the image.
    fn image_surface(
        &self,
        chain: &Chain,
        image: usize,
        extent: (i32, i32),
        context: &mut DirectContext,
    ) -> Result<Surface, Error> {
        let request = chain.swapchain.request();
        let format = SkiaFormat::of(request.format.format)
            .ok_or(Error::SwapchainFormat(request.format.format))?;
        let skia = SkiaImage {
            image: chain.swapchain.images()[image],
            layout: chain.layouts[image],
            format: format.skia,
            usage: request.usage,
            queue_family: self.device.queue_family(),
            conversion: None,
        };
        // SAFETY: Skia receives the swapchain image without a lifetime. The image outlives every use of the
        // description. The render target made from the description is dropped in the frame that wraps the image, and
        // the chain owns the image.
        let info = unsafe { image_info(&skia) }?;
        let backend = gpu::backend_render_targets::make_vk(extent, &info);
        let output_space = crate::color::output_color_space(chain.info.profile)
            .ok_or(Error::OutputColorSpace(chain.info.profile))?;
        gpu::surfaces::wrap_backend_render_target(
            context,
            &backend,
            SurfaceOrigin::TopLeft,
            chain.color,
            Some(output_space),
            None,
        )
        .ok_or_else(|| Error::WrapSwapchainImage(context_state(context)))
    }
}

/// Flushes and submits the frame drawn into `surface`. The flush signals `finished` and moves the image into the
/// layout for presentation.
fn submit(
    context: &mut DirectContext,
    surface: &mut Surface,
    finished: &Semaphore,
    queue_family: u32,
) -> Result<(), Error> {
    // SAFETY: skia-safe has no safe way to hand over a semaphore, because Skia stores the raw handle without a
    // lifetime. The chain owns the semaphore of the image and drops it only when the chain is replaced, after the
    // queue is idle.
    let finished = unsafe { backend_semaphore(finished.handle()) }?;
    let mut signals = [finished];
    let mut flush = FlushInfo::default();
    // SAFETY: skia-safe declares the signal list unsafe, because Skia reads it through a raw pointer. `signals`
    // outlives the flush below, which is the only use of the list.
    let flush = unsafe { flush.set_signal_semaphores(&mut signals) };
    let state =
        skvk::mutable_texture_states::new_vulkan(skvk::ImageLayout::PRESENT_SRC_KHR, queue_family);
    if context.flush_surface_with_texture_state(surface, flush, Some(&state))
        != gpu::SemaphoresSubmitted::Yes
    {
        return Err(Error::PresentationSemaphore);
    }
    if !context.submit(None) {
        return Err(Error::SubmitFrame);
    }
    Ok(())
}

/// The extent of a swapchain as the `i32` width and height expected by Skia.
fn skia_extent(extent: vk::Extent2D) -> Result<(i32, i32), Error> {
    let width = i32::try_from(extent.width).map_err(Error::SwapchainExtent)?;
    let height = i32::try_from(extent.height).map_err(Error::SwapchainExtent)?;
    Ok((width, height))
}

/// The floating-point surface into which the frames of the chain are drawn. The surface is created once and reused
/// across frames for partial redraws.
fn working_surface<'a>(
    working: &'a mut Option<Surface>,
    context: &mut DirectContext,
    extent: (i32, i32),
) -> Result<&'a mut Surface, Error> {
    if working.is_none() {
        let space = crate::color::working_color_space().ok_or(Error::WorkingColorSpace)?;
        let info = skia_safe::ImageInfo::new(
            extent,
            ColorType::RGBAF16,
            skia_safe::AlphaType::Premul,
            space,
        );
        *working = gpu::surfaces::render_target(
            context,
            gpu::Budgeted::Yes,
            &info,
            Some(0),
            Some(SurfaceOrigin::TopLeft),
            None,
            false,
            false,
        );
    }
    working.as_mut().ok_or(Error::WorkingSurface)
}

/// The reference white of the scene in cd/m², the HDR reference white of ITU-R BT.2408.
const SCENE_REFERENCE_WHITE: f64 = 203.0;

/// The reference white of scRGB in cd/m², which IEC 61966-2-2 maps to 1.0.
const SCRGB_REFERENCE_WHITE: f64 = 80.0;

/// The color filter that scales the scene colors to the reference white of the swapchain image for `profile`. Only
/// scRGB has a reference white other than that of the scene, so the other profiles get no filter.
fn output_color_filter(profile: OutputProfile) -> Result<Option<skia_safe::ColorFilter>, Error> {
    if profile != OutputProfile::ScRgb {
        return Ok(None);
    }
    let gain =
        scalar(SCENE_REFERENCE_WHITE / SCRGB_REFERENCE_WHITE).map_err(Error::OutputColorFilter)?;
    Ok(Some(skia_safe::color_filters::matrix_row_major(
        &[
            gain, 0.0, 0.0, 0.0, 0.0, 0.0, gain, 0.0, 0.0, 0.0, 0.0, 0.0, gain, 0.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0,
        ],
        skia_safe::color_filters::Clamp::No,
    )))
}
