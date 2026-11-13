// SPDX-License-Identifier: GPL-3.0-only
use crate::buffer_importer::HardwareBufferImporter;
use crate::error::Error;
use crate::platform::VulkanRenderer;
use crate::surface_stream::SurfaceStream;
use lavashell_core::prelude::{Presented, StreamId};
use lavashell_renderer::{FrameDevice, Latch, LatchedFrames, Painter, Stream, StreamBackend};
use std::collections::BTreeMap;

/// The streams drawn by the AOSP renderer. They exist only on the renderer thread. `AospHost::run_on_streams` runs a
/// task with them.
pub struct AospStreams {
    frames: LatchedFrames<HardwareBufferImporter>,
    attached: BTreeMap<StreamId, Stream<SurfaceStream>>,
}

impl AospStreams {
    pub(crate) fn new(importer: HardwareBufferImporter) -> Self {
        Self {
            frames: LatchedFrames::new(importer),
            attached: BTreeMap::new(),
        }
    }

    /// Adds `stream`, whose images the renderer draws from then on, and returns the id of its images.
    pub fn attach(&mut self, stream: SurfaceStream) -> StreamId {
        let stream = Stream::new(stream);
        let id = stream.id();
        let _fresh_id_replaces_nothing = self.attached.insert(id, stream);
        id
    }

    /// Removes the stream `id`. The producer receives its frames back once the GPU has finished reading them.
    pub fn detach(&mut self, id: StreamId) -> Option<Stream<SurfaceStream>> {
        self.attached.remove(&id)
    }

    /// # Errors
    /// Fails when no stream `id` is attached.
    pub fn stream(&mut self, id: StreamId) -> Result<&mut Stream<SurfaceStream>, Error> {
        find(&mut self.attached, id)
    }

    /// Latches the newest frame queued by the producer on the stream `id` and releases the frame latched before.
    /// `painter` records a frame that cannot be drawn as a draw failure, and the latch returns it as `Latch::Rejected`.
    /// The method returns `Latch::Unchanged` while the producer is still writing the newest frame. Compose the view
    /// that draws the returned image after the latch. A frame skips images older than the image latched last.
    ///
    /// # Errors
    /// Fails when no stream `id` is attached, when AOSP cannot return the frame or the id of a buffer freed by the
    /// producer, or when Vulkan cannot read the buffer of the frame or the completion of the renders that drew earlier
    /// frames. After such a failure, detach the stream and give the producer a new one.
    pub fn latch(&mut self, painter: &Painter, id: StreamId) -> Result<Latch, Error> {
        self.frames.latch(painter, find(&mut self.attached, id)?)
    }

    /// The format Vulkan reports for `buffer`. It contains the suggested YCbCr model, range and chroma offsets. An
    /// application may replace these defaults when it samples the buffer.
    ///
    /// # Errors
    /// Fails when Vulkan cannot read the properties of the buffer.
    #[cfg(target_os = "android")]
    pub fn buffer_format(
        &self,
        buffer: &crate::platform::ndk::hardware_buffer::HardwareBuffer,
    ) -> Result<lavashell_renderer::ForeignFormat, Error> {
        self.frames.importer().buffer_format(buffer)
    }
}

impl StreamBackend for AospStreams {
    type Gpu = VulkanRenderer;
    type Error = Error;

    fn draw<T>(
        &mut self,
        gpu: &mut VulkanRenderer,
        painter: &mut Painter,
        tree: Presented<'_>,
        draw: impl FnOnce(FrameDevice<'_, VulkanRenderer>) -> T,
    ) -> (T, Result<(), Error>) {
        self.frames.draw(gpu, painter, tree, draw)
    }
}

/// The stream `id` among `attached`.
fn find(
    attached: &mut BTreeMap<StreamId, Stream<SurfaceStream>>,
    id: StreamId,
) -> Result<&mut Stream<SurfaceStream>, Error> {
    attached.get_mut(&id).ok_or(Error::FindStream(id))
}
