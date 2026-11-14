// SPDX-License-Identifier: GPL-3.0-only
//! Frames queued by a producer into a stream. The renderer latches the newest frame of a stream, imports the frame
//! when a paint first draws it and returns it to the producer once the GPU has finished every render that reads it.

mod latched;
mod release;

pub use latched::LatchedFrames;

use crate::error::MultipleFailures;
use crate::vulkan::{Status, VulkanRenderer};
use crate::{Painter, Sampling, StreamFrames, StreamPixels, StreamRejection};
use lavashell_core::prelude::{PixelSize, Presented, StreamId, StreamImage};
use skia_safe::IPoint;
use std::convert::Infallible;
use std::os::fd::OwnedFd;
use std::rc::Rc;

/// The result of a latch on a stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Latch {
    /// A new frame was latched. Drawing this image draws that frame.
    Frame(StreamImage),
    /// The producer has not queued a frame since the last latch, or the producer is still writing the queued frame.
    Unchanged,
    /// The newest frame was released without being drawn. Renders draw the frame latched before.
    Rejected(StreamRejection),
}

/// The newest frame of a source.
pub enum Taken<F> {
    /// The producer has not queued a frame since the last take, or the producer is still writing the queued frame.
    Nothing,
    Frame(F),
    /// The source returned the newest frame to the producer without handing it out.
    Rejected(StreamRejection),
}

/// A frame written by a producer into a buffer of a queue.
pub trait ProducedFrame {
    /// Identifies the buffer. A later frame in the same buffer reuses the import of the buffer.
    type BufferKey: Ord + Copy;
    type Queue: BufferQueue + Clone;

    fn buffer_key(&self) -> Self::BufferKey;

    /// The queue that delivered the buffer.
    fn queue(&self) -> &Self::Queue;

    /// The top left corner of the part of the buffer shown by the frame.
    fn origin(&self) -> IPoint;

    /// The size of the part of the buffer shown by the frame.
    fn size(&self) -> PixelSize;

    /// Returns the buffer to the producer once `after` signals. `None` means that no GPU work reads the buffer.
    fn release(self, after: Option<OwnedFd>);
}

pub trait BufferQueue {
    /// Whether the queue exists. A deleted queue frees its buffers without reporting them in
    /// `FrameSource::take_removed_buffers`.
    fn exists(&self) -> bool;
}

/// Lets the renderer call the latch protocol of a `FrameSource`. Only the renderer can create one.
pub struct LatchAccess(());

/// The consumer end of the queues into which a producer writes frames.
pub trait FrameSource {
    type Frame: ProducedFrame;
    type Error: std::error::Error + 'static;

    /// Takes the newest frame finished by the producer. Every older frame returns to the producer.
    ///
    /// # Errors
    /// Fails when the source cannot read its queued frames.
    fn take_newest(&mut self, access: &LatchAccess) -> Result<Taken<Self::Frame>, Self::Error>;

    /// Records that `frame`, taken from this source, is drawn in place of the frame latched before. The frame
    /// latched before has already been released.
    fn record_latch(&mut self, access: &LatchAccess, frame: &Self::Frame);

    /// Takes the buffers freed by the producers since the last call. An item is an error when a freed buffer cannot
    /// be identified.
    fn take_removed_buffers(
        &mut self,
        access: &LatchAccess,
    ) -> impl Iterator<Item = Result<<Self::Frame as ProducedFrame>::BufferKey, Self::Error>>;
}

/// Submits and reads completions for the GPU work of a renderer. No method waits for the GPU.
pub trait Completions {
    /// Signals once GPU work submitted before it is complete.
    type Completion;
    type Error: std::error::Error + MultipleFailures + From<FenceCopyError> + 'static;

    /// # Errors
    /// Fails when the GPU cannot submit the completion.
    fn completion(&self) -> Result<Self::Completion, Self::Error>;

    /// # Errors
    /// Fails when the GPU cannot report the status, such as after the loss of the device.
    fn status(&self, completion: &Self::Completion) -> Result<Status, Self::Error>;
}

/// Imports frames into the GPU of a renderer and hands them back to the producers that wrote them once the GPU has
/// read them. No method waits for the GPU.
pub trait FrameImporter: Completions {
    /// The renderer whose GPU reads the imports.
    type Gpu;
    type Frame: ProducedFrame;
    /// A frame in the form imported by the importer.
    type Sampled;
    type Import;

    /// Determines how the renderer samples `frame`, without GPU work.
    ///
    /// # Errors
    /// Fails when the importer cannot read the properties of the frame.
    fn sample(&self, frame: &Self::Frame) -> Result<Sampling<Self::Sampled>, Self::Error>;

    /// # Errors
    /// Fails when the GPU cannot import the frame.
    fn import(&self, sampled: &Self::Sampled) -> Result<Self::Import, Self::Error>;

    /// The pixels of `import` for the paints. The pixels have a reference to `import`.
    ///
    /// # Errors
    /// Fails when the painter cannot wrap the import.
    fn acquire(
        &self,
        gpu: &mut Self::Gpu,
        import: &Rc<Self::Import>,
    ) -> Result<StreamPixels, Self::Error>;

    /// Hands `imports` back to the producers that wrote them. Returns a fence that signals once the GPU has finished
    /// the work submitted so far, `None` when that work is already finished.
    ///
    /// # Errors
    /// Fails when the GPU cannot hand an import back or cannot signal the fence.
    fn release(
        &mut self,
        gpu: &mut Self::Gpu,
        imports: &[Rc<Self::Import>],
    ) -> Result<Option<OwnedFd>, Self::Error>;

    /// Destroys the dropped imports no longer read by the GPU.
    ///
    /// # Errors
    /// Fails when the GPU cannot report the status of its work or cannot submit a completion.
    fn poll(&mut self) -> Result<(), Self::Error>;
}

/// What a stream backend hands the draw of a frame: the GPU, the painter and the frames bound for the paints.
pub struct FrameDevice<'a, G> {
    pub gpu: &'a mut G,
    pub painter: &'a mut Painter,
    pub streams: &'a StreamFrames,
}

/// The frames of streams, drawn by the renderer together with each frame it paints.
pub trait StreamBackend {
    /// The renderer whose GPU reads the frames.
    type Gpu;
    type Error: std::error::Error + 'static;

    /// Runs `draw` with the frames bound for the paints. The latched frames that `tree` draws are imported before
    /// `draw`, and the frames that the GPU has finished reading return to the streams that produced them after
    /// `draw`. Returns the result of `draw` together with the failures of the streams, which do not stop `draw`.
    fn draw<T>(
        &mut self,
        gpu: &mut Self::Gpu,
        painter: &mut Painter,
        tree: Presented<'_>,
        draw: impl FnOnce(FrameDevice<'_, Self::Gpu>) -> T,
    ) -> (T, Result<(), Self::Error>);
}

/// Streams that a renderer on its own thread can draw. They use `VulkanRenderer`, and their errors can be sent to
/// another thread.
pub trait RendererStreams: StreamBackend<Gpu = VulkanRenderer, Error: Send> + 'static {}

impl<S: StreamBackend<Gpu = VulkanRenderer, Error: Send> + 'static> RendererStreams for S {}

/// The streams for a renderer that draws only frames without streams.
#[derive(Default)]
pub struct NoStreams {
    frames: StreamFrames,
}

impl StreamBackend for NoStreams {
    type Gpu = VulkanRenderer;
    type Error = Infallible;

    fn draw<T>(
        &mut self,
        gpu: &mut Self::Gpu,
        painter: &mut Painter,
        _: Presented<'_>,
        draw: impl FnOnce(FrameDevice<'_, Self::Gpu>) -> T,
    ) -> (T, Result<(), Infallible>) {
        let device = FrameDevice {
            gpu,
            painter,
            streams: &self.frames,
        };
        (draw(device), Ok(()))
    }
}

/// A failure to copy the release fence for a frame from a stream.
#[derive(Debug, thiserror::Error)]
#[error("cannot copy the release fence for a frame from a stream")]
pub struct FenceCopyError(#[source] std::io::Error);

/// A source of frames and the image of the frame latched from it last.
pub struct Stream<S> {
    id: StreamId,
    source: S,
    latest: Option<StreamImage>,
}

impl<S: FrameSource> Stream<S> {
    #[must_use]
    pub fn new(source: S) -> Self {
        Self {
            id: StreamId::next(),
            source,
            latest: None,
        }
    }

    /// The stream of every image latched from this source.
    #[must_use]
    pub fn id(&self) -> StreamId {
        self.id
    }

    #[must_use]
    pub fn source(&self) -> &S {
        &self.source
    }

    pub fn source_mut(&mut self) -> &mut S {
        &mut self.source
    }

    /// The size in pixels of the frame latched last.
    #[must_use]
    pub fn drawn_size(&self) -> Option<PixelSize> {
        self.latest.map(|image| image.size)
    }

    /// Returns the image that draws `frame`, latched in place of the frame latched before.
    fn next_image(&mut self, frame: &S::Frame) -> StreamImage {
        let generation = self.latest.map_or(1, |image| image.generation + 1);
        let image = StreamImage {
            stream: self.id,
            generation,
            size: frame.size(),
        };
        self.latest = Some(image);
        image
    }
}
