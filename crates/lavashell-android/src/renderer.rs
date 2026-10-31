// SPDX-License-Identifier: GPL-3.0-only
use crate::error::{Error, Failures};
use crate::latched_frames::{Latch, LatchedFrames};
use crate::native_window::NativeWindow;
use crate::platform::{SurfaceTarget, VulkanRenderer};
use crate::surface_stream::SurfaceStream;
use lavashell_core::prelude::{Density, OutputInfo, OutputRequest, Presented};
use lavashell_renderer::{Fonts, FrameOutcome, Painter, SurfaceAlpha, skia};

struct Attachment {
    target: SurfaceTarget,
    window: NativeWindow,
}

/// A Vulkan renderer for AOSP that draws into one Surface at a time and can attach to another Surface later. Every
/// call on it must come from the same thread.
pub struct AndroidRenderer {
    attachment: Option<Attachment>,
    painter: Painter,
    gpu: VulkanRenderer,
    // Declared last, so that the latched frames are dropped after `painter` and `gpu`. By then the GPU is idle and
    // Skia has released the images of the frames.
    streams: LatchedFrames,
}

impl AndroidRenderer {
    /// # Errors
    /// Fails when no Vulkan device suits the renderer, the device cannot import the frames of streams, or the painter
    /// cannot start.
    pub fn new(fonts: &Fonts) -> Result<Self, Error> {
        let gpu = VulkanRenderer::for_android().map_err(Error::CreateRenderer)?;
        let painter = gpu.painter(fonts).map_err(Error::CreatePainter)?;
        Ok(Self {
            attachment: None,
            painter,
            streams: LatchedFrames::new(&gpu)?,
            gpu,
        })
    }

    /// Latches the newest frame queued by the producer on `stream` and releases the frame latched before. The released
    /// frame returns to the producer once the GPU has finished every render that could draw that frame. Returns the
    /// image that draws the new frame, `Latch::Unchanged` when the producer has queued no frame since the last latch,
    /// or `Latch::Rejected` with the reason why the newest frame is not drawn.
    ///
    /// The latch does not block on a frame that the producer is still writing. That frame waits in the stream, and
    /// renders draw the frame latched before. Latch again on a later render while `SurfaceStream::waits_for_producer`
    /// is true.
    ///
    /// A frame that cannot be drawn is released, returned as `Latch::Rejected` and recorded as a draw failure. Renders
    /// then draw the frame latched before. A latch does no GPU work, because a render imports the frame when the
    /// render draws it. A frame that fails that import is recorded as a draw failure, and the stream then draws
    /// nothing. Latch the streams before laying out the View and draw the image with `Drawing::stream_image`. A render
    /// that draws an image of the stream older than the one latched last draws nothing for the stream.
    ///
    /// # Errors
    /// Fails when AOSP cannot return the frame or the id of a buffer freed by the producer, or when Vulkan cannot read
    /// the buffer of the frame or the completion of the renders that drew earlier frames. After such a failure, drop
    /// the stream and give the producer a new one. After the loss of the Vulkan device, recreate the renderer.
    pub fn latch(&mut self, stream: &mut SurfaceStream) -> Result<Latch, Error> {
        self.streams.latch(&self.painter, stream)
    }

    /// The format reported for `buffer` by the Vulkan device used by the renderer. It includes the suggested YCbCr
    /// model, range and chroma offsets, which Vulkan reports as defaults that an application may replace when it
    /// samples the buffer.
    ///
    /// # Errors
    /// Fails when Vulkan cannot read the properties of the buffer.
    #[cfg(target_os = "android")]
    pub fn buffer_format(
        &self,
        buffer: &crate::platform::ndk::hardware_buffer::HardwareBuffer,
    ) -> Result<lavashell_renderer::ForeignFormat, Error> {
        self.streams.buffer_format(buffer)
    }

    /// Replaces the surface that the renderer presents to with `window`. The renderer is the only producer connected to
    /// the window until `detach`, the next `attach` or the drop of this renderer.
    ///
    /// # Errors
    /// Fails when the device cannot present to the window. The renderer is then detached.
    pub fn attach(&mut self, window: NativeWindow, alpha: SurfaceAlpha) -> Result<(), Error> {
        self.attachment = None;
        let target =
            self.gpu
                .native_window_target(window.window.clone(), alpha, window.timing.clone());
        let target = target.map_err(Error::Attach)?;
        self.attachment = Some(Attachment { target, window });
        Ok(())
    }

    /// Finishes the queued GPU work and releases the surface. Call it before the consumer of the surface closes. The
    /// returned `NativeWindow` contains the timing history, so that a later `attach` continues that history.
    pub fn detach(&mut self) -> Option<NativeWindow> {
        self.attachment.take().map(|attachment| attachment.window)
    }

    /// The width and the height of the consumer of the attached window in pixels, or `None` while the renderer is
    /// detached. A buffer geometry set explicitly on the window can differ from this size.
    ///
    /// # Errors
    /// Fails as `NativeWindow::size` does.
    pub fn size(&self) -> Result<Option<(u32, u32)>, Error> {
        self.attachment
            .as_ref()
            .map(|a| a.window.size())
            .transpose()
    }

    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.gpu.device_name
    }

    #[must_use]
    pub fn supports_presentation_time(&self) -> bool {
        self.gpu.supports_presentation_time()
    }

    pub fn painter(&mut self) -> &mut Painter {
        &mut self.painter
    }

    #[must_use]
    pub fn output(&self) -> Option<OutputInfo> {
        self.attachment.as_ref().and_then(|a| a.target.output())
    }

    /// # Errors
    /// Fails when no native window is attached.
    pub fn request_output(&mut self, request: OutputRequest) -> Result<(), Error> {
        let attachment = self.attachment.as_mut().ok_or(Error::Detached)?;
        attachment.target.request_output(request);
        Ok(())
    }

    /// Paints a complete frame of `tree`, converting dp to pixels with `density`. A detached renderer or a surface of
    /// zero size returns `FrameOutcome::Pending` without submitting GPU work.
    ///
    /// # Errors
    /// Fails when the frame cannot be drawn, or when the frames of streams cannot be imported or returned to their
    /// producers. When the frame is drawn but one of its streams fails, the error is `Error::StreamsOfDrawnFrame`,
    /// which contains the outcome of the frame. After the loss of the Vulkan device, recreate the renderer. After
    /// another failure of the streams, drop the streams drawn by the render.
    pub fn render(
        &mut self,
        tree: Presented<'_>,
        density: Density,
        clear: skia::Color,
    ) -> Result<FrameOutcome, Error> {
        self.render_frame(tree, density, clear, None)
    }

    /// Paints like `render` and sets `presentation_time_ns` as the timestamp of the buffer, as described at
    /// `VulkanRenderer::draw_at`. An attached surface needs `supports_presentation_time`. A detached renderer returns
    /// `FrameOutcome::Pending`. On a device without presentation times, the call fails before it acquires an image.
    /// The timestamp is discarded when the frame is `Pending`.
    ///
    /// # Errors
    /// Fails as `render` does.
    pub fn render_at(
        &mut self,
        tree: Presented<'_>,
        density: Density,
        clear: skia::Color,
        presentation_time_ns: u64,
    ) -> Result<FrameOutcome, Error> {
        self.render_frame(tree, density, clear, Some(presentation_time_ns))
    }

    fn render_frame(
        &mut self,
        tree: Presented<'_>,
        density: Density,
        clear: skia::Color,
        presentation_time_ns: Option<u64>,
    ) -> Result<FrameOutcome, Error> {
        let mut failures = Failures::default();
        failures.add(self.streams.import(&mut self.gpu, &self.painter, tree));
        let (painter, frames) = (&mut self.painter, self.streams.frames());
        let drawn = Self::draw(
            &mut self.gpu,
            &mut self.attachment,
            presentation_time_ns,
            |canvas| painter.paint_scene_with_streams(canvas.clear(clear), tree, density, frames),
        );
        // A stream that failed to import draws nothing. The other streams draw as usual.
        failures.add(self.streams.release(&mut self.gpu));
        match (drawn, failures.into_result()) {
            (Ok(frame), Ok(())) => Ok(frame),
            (Ok(outcome), Err(streams)) => Err(Error::StreamsOfDrawnFrame {
                outcome,
                streams: Box::new(streams),
            }),
            (Err(failure), Ok(())) => Err(failure),
            (Err(drawing), Err(streams)) => Err(drawing.then(streams)),
        }
    }

    /// Draws Skia operations in physical pixels.
    ///
    /// # Errors
    /// Fails when the frame cannot be drawn.
    pub fn draw_canvas(&mut self, draw: impl FnOnce(&skia::Canvas)) -> Result<FrameOutcome, Error> {
        Self::draw(&mut self.gpu, &mut self.attachment, None, draw)
    }

    fn draw(
        gpu: &mut VulkanRenderer,
        attachment: &mut Option<Attachment>,
        presentation_time_ns: Option<u64>,
        draw: impl FnOnce(&skia::Canvas),
    ) -> Result<FrameOutcome, Error> {
        let Some(attachment) = attachment else {
            return Ok(FrameOutcome::Pending);
        };
        let size = attachment.window.size()?;
        let drawn = match presentation_time_ns {
            Some(time) => gpu.draw_at(&mut attachment.target, size, time, draw),
            None => gpu.draw(&mut attachment.target, size, draw),
        };
        drawn.map_err(Error::Draw)
    }
}
