// SPDX-License-Identifier: GPL-3.0-only
use super::extent::ExtentChange;
use super::{
    Chain, DrawnFrame, FrameOutcome, PresentationTime, SkippedFrame, SurfaceTarget, present,
};
use crate::Error;
use crate::vulkan::skia_handoff::{
    SkiaImage, backend_semaphore, context_state, gpu_render_target, image_info,
};
use ash::vk;
use lavashell_core::prelude::{OutputProfile, PixelSize};
use lavashell_renderer::scalar;
use lavashell_vulkan::{Acquisition, Fit, Presented, Semaphore};
use skia_safe::gpu::{self, DirectContext, FlushInfo, SurfaceOrigin, vk as skvk};
use skia_safe::{Canvas, Surface};

impl SurfaceTarget {
    pub(in crate::vulkan) fn draw<T>(
        &mut self,
        context: &mut DirectContext,
        size: PixelSize,
        presentation_time: Option<PresentationTime>,
        draw: impl FnOnce(&Canvas) -> T,
    ) -> Result<DrawnFrame<T>, Error> {
        present::before_acquisition(self.presentation, size, presentation_time, || {
            self.draw_ready(context, size, presentation_time, draw)
        })
    }

    fn draw_ready<T>(
        &mut self,
        context: &mut DirectContext,
        size: PixelSize,
        presentation_time: Option<PresentationTime>,
        draw: impl FnOnce(&Canvas) -> T,
    ) -> Result<DrawnFrame<T>, Error> {
        let mut chain = self.current_chain(size)?;
        let drawn = self.draw_chain(&mut chain, context, presentation_time, draw);
        self.chain = Some(chain);
        drawn.inspect_err(|_| self.stale = true)
    }

    fn current_chain(&mut self, size: PixelSize) -> Result<Chain, Error> {
        if self.extent_change()? == ExtentChange::Changed {
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

    fn draw_chain<T>(
        &mut self,
        chain: &mut Chain,
        context: &mut DirectContext,
        presentation_time: Option<PresentationTime>,
        draw: impl FnOnce(&Canvas) -> T,
    ) -> Result<DrawnFrame<T>, Error> {
        let slot = chain.next_slot;
        if let Some(frame) = &chain.slots[slot].frame {
            frame.wait().map_err(Error::WaitFrame)?;
        }
        // SAFETY: the swapchain API of lavashell-vulkan has no safe acquisition, because only the caller can track the
        // state of the semaphore and of the work that uses the image. The semaphore of the slot has no signal or wait
        // pending. An acquisition that returned no image signaled nothing. The last image acquired with this
        // semaphore was waited for by the Skia flush of its frame. The completion of that frame, stored in the slot,
        // was submitted after the flush and has completed. A frame that failed in between made the chain stale, so
        // the chain and its slots were replaced. The chain declares its slots before its swapchain, so every frame
        // that uses an acquired image completes before the swapchain drops.
        let acquired = unsafe { chain.swapchain.acquire(&chain.slots[slot].acquire) }
            .map_err(Error::Acquire)?;
        let (index, acquired_fit) = match acquired {
            Acquisition::Image { index, fit } => (index, fit),
            Acquisition::NotReady => return Ok(DrawnFrame::Skipped(SkippedFrame::ImageNotReady)),
            Acquisition::OutOfDate => {
                self.stale = true;
                return Ok(DrawnFrame::Skipped(SkippedFrame::ChainOutOfDate));
            }
        };
        let image = usize::try_from(index).map_err(Error::SwapchainImageIndex)?;
        let extent = skia_extent(chain.swapchain.request().extent)?;
        let mut surface = self.image_surface(chain, image, extent, context)?;
        // SAFETY: skia-safe has no safe way to hand over a semaphore, because Skia stores the raw handle without a
        // lifetime. The slot owns the semaphore, and the completion of this frame uses it until the fence of that
        // completion signals. That fence follows the wait.
        let acquisition = unsafe { backend_semaphore(chain.slots[slot].acquire.handle()) }?;
        if !surface.wait(&[acquisition], false) {
            return Err(Error::WaitAcquisition);
        }
        let working = working_surface(&mut chain.working, context, extent)?;
        let painted = draw(working.canvas());
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
        let acquire = chain.slots[slot].acquire.clone();
        chain.slots[slot].frame = Some(
            (self.device.signal())
                .map_err(Error::SignalFrame)?
                .holding(acquire),
        );
        chain.layouts[image] = skvk::ImageLayout::PRESENT_SRC_KHR;
        chain.next_slot = (chain.next_slot + 1) % chain.slots.len();
        self.presentation = self.presentation.after_submission(presentation_time);
        let time = presentation_time.map(PresentationTime::time);
        // SAFETY: the swapchain API of lavashell-vulkan has no safe presentation, because only the caller can track the
        // acquisition and the semaphore. Image `index` was acquired above and is presented once. The flush above
        // submitted the signal of the semaphore of the image after the writes to the image. Only this presentation
        // waits on that signal. A later frame signals the semaphore again only after it acquires the image anew, and
        // the presentation engine returns the image for a new acquisition only after this wait is consumed.
        let presented = unsafe { chain.swapchain.present(index, &chain.finished[image], time) };
        let outcome = match presented.map_err(Error::Present)? {
            Presented::Shown(fit) => {
                let suboptimal = acquired_fit == Fit::Suboptimal || fit == Fit::Suboptimal;
                self.stale = suboptimal && self.transform.is_none();
                FrameOutcome::Shown
            }
            Presented::OutOfDate => {
                self.stale = true;
                FrameOutcome::Stale
            }
        };
        Ok(DrawnFrame::Drawn { outcome, painted })
    }

    fn image_surface(
        &self,
        chain: &Chain,
        image: usize,
        extent: (i32, i32),
        context: &mut DirectContext,
    ) -> Result<Surface, Error> {
        let skia = SkiaImage {
            image: chain.swapchain.images()[image],
            layout: chain.layouts[image],
            format: chain.format.skia(),
            usage: chain.swapchain.request().usage,
            queue_family: self.device.queue_family(),
            conversion: None,
        };
        // SAFETY: Skia receives the swapchain image without a lifetime. The image outlives every use of the
        // description. The render target made from the description is dropped in the frame that wraps the image, and
        // the chain owns the image.
        let info = unsafe { image_info(&skia) }?;
        let backend = gpu::backend_render_targets::make_vk(extent, &info);
        let output_space = lavashell_renderer::color::output_color_space(chain.info.profile)
            .ok_or(Error::OutputColorSpace(chain.info.profile))?;
        gpu::surfaces::wrap_backend_render_target(
            context,
            &backend,
            SurfaceOrigin::TopLeft,
            chain.format.color_type(),
            Some(output_space),
            None,
        )
        .ok_or_else(|| Error::WrapSwapchainImage(context_state(context)))
    }
}

fn submit(
    context: &mut DirectContext,
    surface: &mut Surface,
    finished: &Semaphore,
    queue_family: u32,
) -> Result<(), Error> {
    // SAFETY: skia-safe has no safe way to hand over a semaphore, because Skia stores the raw handle without a
    // lifetime. The chain owns the semaphore of the image. It drops the semaphore after its slots, whose frames wait
    // for this flush, and the swapchain uses the semaphore until the presentation that waits on it finishes.
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

fn skia_extent(extent: vk::Extent2D) -> Result<(i32, i32), Error> {
    let width = i32::try_from(extent.width).map_err(Error::SwapchainExtent)?;
    let height = i32::try_from(extent.height).map_err(Error::SwapchainExtent)?;
    Ok((width, height))
}

fn working_surface<'a>(
    working: &'a mut Option<Surface>,
    context: &mut DirectContext,
    extent: (i32, i32),
) -> Result<&'a mut Surface, Error> {
    if working.is_none() {
        let info = lavashell_renderer::color::working_image_info(extent)
            .map_err(Error::WorkingImageInfo)?;
        *working = gpu_render_target(context, &info, gpu::Budgeted::Yes);
    }
    working.as_mut().ok_or(Error::WorkingSurface)
}

// https://www.itu.int/pub/R-REP-BT.2408
const BT2408_HDR_REFERENCE_WHITE_CD_PER_M2: f64 = 203.0;

// https://webstore.iec.ch/en/publication/6171
const SCRGB_REFERENCE_WHITE_CD_PER_M2: f64 = 80.0;

fn output_color_filter(profile: OutputProfile) -> Result<Option<skia_safe::ColorFilter>, Error> {
    if profile != OutputProfile::ScRgb {
        return Ok(None);
    }
    let gain = scalar(BT2408_HDR_REFERENCE_WHITE_CD_PER_M2 / SCRGB_REFERENCE_WHITE_CD_PER_M2)
        .map_err(Error::OutputColorFilter)?;
    Ok(Some(skia_safe::color_filters::matrix_row_major(
        &[
            gain, 0.0, 0.0, 0.0, 0.0, 0.0, gain, 0.0, 0.0, 0.0, 0.0, 0.0, gain, 0.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0,
        ],
        skia_safe::color_filters::Clamp::No,
    )))
}
