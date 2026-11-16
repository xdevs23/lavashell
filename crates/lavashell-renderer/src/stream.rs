// SPDX-License-Identifier: GPL-3.0-only

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Latch {
    Frame(StreamImage),
    Unchanged,
    Rejected(StreamRejection),
}

pub enum Taken<F> {
    Nothing,
    Frame(F),
    Rejected(StreamRejection),
}

pub trait ProducedFrame {
    type BufferKey: Ord + Copy;
    type Queue: BufferQueue + Clone;

    fn buffer_key(&self) -> Self::BufferKey;

    fn queue(&self) -> &Self::Queue;

    fn origin(&self) -> IPoint;

    fn size(&self) -> PixelSize;

    fn release(self, after: Option<OwnedFd>);
}

pub trait BufferQueue {
    fn exists(&self) -> bool;
}

pub struct LatchAccess(());

pub trait FrameSource {
    type Frame: ProducedFrame;
    type Error: std::error::Error + 'static;

    fn take_newest(&mut self, access: &LatchAccess) -> Result<Taken<Self::Frame>, Self::Error>;

    fn record_latch(&mut self, access: &LatchAccess, frame: &Self::Frame);

    fn take_removed_buffers(
        &mut self,
        access: &LatchAccess,
    ) -> impl Iterator<Item = Result<<Self::Frame as ProducedFrame>::BufferKey, Self::Error>>;
}

pub trait Completions {
    type Completion;
    type Error: std::error::Error + MultipleFailures + From<FenceCopyError> + 'static;

    fn completion(&self) -> Result<Self::Completion, Self::Error>;

    fn status(&self, completion: &Self::Completion) -> Result<Status, Self::Error>;
}

pub trait FrameSampler: Completions {
    type Frame: ProducedFrame;
    type Sampled;

    fn sample(&self, frame: &Self::Frame) -> Result<Sampling<Self::Sampled>, Self::Error>;
}

pub trait FrameImporter: FrameSampler {
    type Gpu;
    type Import;

    fn import(&self, sampled: &Self::Sampled) -> Result<Self::Import, Self::Error>;

    fn acquire(
        &self,
        gpu: &mut Self::Gpu,
        import: &Rc<Self::Import>,
    ) -> Result<StreamPixels, Self::Error>;
}

pub trait ImportHandBack: FrameImporter {
    fn release(
        &mut self,
        gpu: &mut Self::Gpu,
        imports: &[Rc<Self::Import>],
    ) -> Result<Option<OwnedFd>, Self::Error>;

    fn poll(&mut self) -> Result<(), Self::Error>;
}

pub struct FrameDevice<'a, G> {
    pub gpu: &'a mut G,
    pub painter: &'a mut Painter,
    pub streams: &'a StreamFrames,
}

pub trait StreamBackend {
    type Gpu;
    type Error: std::error::Error + 'static;

    fn draw<T>(
        &mut self,
        gpu: &mut Self::Gpu,
        painter: &mut Painter,
        tree: Presented<'_>,
        draw: impl FnOnce(FrameDevice<'_, Self::Gpu>) -> T,
    ) -> (T, Result<(), Self::Error>);
}

pub trait RendererStreams: StreamBackend<Gpu = VulkanRenderer, Error: Send> + 'static {}

impl<S: StreamBackend<Gpu = VulkanRenderer, Error: Send> + 'static> RendererStreams for S {}

#[derive(Default)]
pub struct NoStreams {
    frames: StreamFrames,
}

#[cfg(test)]
impl NoStreams {
    pub(crate) fn frames_mut(&mut self) -> &mut StreamFrames {
        &mut self.frames
    }
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

#[derive(Debug, thiserror::Error)]
#[error("cannot copy the release fence for a frame from a stream")]
pub struct FenceCopyError(#[source] std::io::Error);

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

    #[must_use]
    pub fn drawn_size(&self) -> Option<PixelSize> {
        self.latest.map(|image| image.size)
    }

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
