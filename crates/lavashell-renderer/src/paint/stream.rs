// SPDX-License-Identifier: GPL-3.0-only
#[cfg(test)]
mod gpu_tests;
#[cfg(test)]
mod tests;

use super::Painter;
use super::image::draw_framed;
use crate::failure::DrawFailure;
use lavashell_core::prelude::*;
use skia_safe::{Canvas, IPoint, IRect};
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::rc::Rc;

#[derive(Default)]
pub struct StreamFrames {
    bound: BTreeMap<StreamId, BoundFrame>,
}

struct BoundFrame {
    generation: u64,
    pixels: StreamPixels,
    origin: IPoint,
}

/// The pixels of a stream frame, together with the value whose drop frees the memory that the pixels borrow.
pub struct StreamPixels {
    /// Declared before `_owner`, so the image drops before the memory it borrows.
    pub(crate) image: skia_safe::Image,
    pub(crate) _owner: Option<Rc<dyn Any>>,
}

impl From<skia_safe::Image> for StreamPixels {
    /// Pixels in memory that Skia owns, such as a raster image or the snapshot of a surface.
    fn from(image: skia_safe::Image) -> Self {
        Self {
            image,
            _owner: None,
        }
    }
}

impl StreamFrames {
    /// Binds `pixels` to `image.stream`. Until the next bind or retire of that stream, paints draw
    /// `image` from the part of `pixels` at `origin` with the size of `image`.
    pub fn bind(&mut self, image: &StreamImage, pixels: StreamPixels, origin: IPoint) {
        let frame = BoundFrame {
            generation: image.generation,
            pixels,
            origin,
        };
        let _replaced_frame = self.bound.insert(image.stream, frame);
    }

    /// Unbinds `stream`, so paints draw nothing for it until the next bind.
    pub fn retire(&mut self, stream: StreamId) {
        let _retired_frame = self.bound.remove(&stream);
    }

    pub(super) fn draw(
        &self,
        canvas: &Canvas,
        image: &StreamImage,
        destination: Rect,
        frame: Rect,
    ) -> Result<(), DrawFailure> {
        let bound = (self.bound.get(&image.stream)).ok_or(DrawFailure::StreamUnbound)?;
        if bound.generation != image.generation {
            return Err(DrawFailure::StreamStale);
        }
        let (width, height) = image.size.signed().ok_or(DrawFailure::ImageSize)?;
        let crop = IRect::from_xywh(bound.origin.x, bound.origin.y, width, height);
        draw_framed(canvas, &bound.pixels.image, crop, destination, frame)
    }
}

pub struct HardwareBufferDescription {
    pub size: PixelSize,
    pub layers: u32,
    pub usage: BTreeSet<BufferFlag>,
    pub alpha: BufferAlpha,
}

/// A usage flag of a hardware buffer that affects whether the import can sample the buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BufferFlag {
    GpuSampled,
    Protected,
    MipmapComplete,
}

/// What the alpha channel of a buffer means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferAlpha {
    Premultiplied,
    /// The format has an alpha channel without meaning, such as R8G8B8X8.
    Undefined,
}

impl HardwareBufferDescription {
    /// The rejection of a buffer that the import cannot sample: a buffer with several layers, a
    /// complete mipmap chain, protected content or without GPU sampled usage.
    #[must_use]
    pub fn rejection(&self) -> Option<StreamRejection> {
        let usage = &self.usage;
        let sampled = self.layers == 1 && usage.contains(&BufferFlag::GpuSampled);
        (!sampled
            || usage.contains(&BufferFlag::Protected)
            || usage.contains(&BufferFlag::MipmapComplete))
        .then_some(StreamRejection::UnusableBuffer)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnsampledFormat {
    /// The format needs a YCbCr conversion, which the device does not support.
    NeedsYcbcrConversion,
}

impl fmt::Display for UnsampledFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NeedsYcbcrConversion => f.write_str(
                "the format needs a YCbCr conversion, which the device does not support; let the \
                 producer draw buffers in RGBA or RGBX with 8 bits per channel",
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StreamRejection {
    TooLarge,
    /// The buffer has several layers, a complete mipmap chain, protected content or no GPU sampled
    /// usage, or the driver cannot describe it.
    UnusableBuffer,
    BufferFormat(UnsampledFormat),
    CropOutside,
    ImportFailed,
}

impl fmt::Display for StreamRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let released =
            "so it is released without being drawn and the stream still shows the previous frame";
        match self {
            Self::TooLarge => write!(
                f,
                "a stream frame is larger than the maximum size set for the stream, {released}; \
                 raise the maximum size of the stream or let the producer draw smaller buffers"
            ),
            Self::UnusableBuffer => write!(
                f,
                "a stream frame has a buffer with several layers, a complete mipmap chain, \
                 protected content or without GPU sampled usage, or a buffer that the driver \
                 cannot describe, {released}; let the producer draw buffers of one layer and one \
                 mipmap level with GPU sampled usage and unprotected content"
            ),
            Self::BufferFormat(reason) => write!(
                f,
                "a stream frame has a buffer format not sampled by the Vulkan device, {released}; \
                 {reason}"
            ),
            Self::CropOutside => write!(
                f,
                "a stream frame has a crop outside its buffer, {released}; let the producer set a \
                 crop within the buffer"
            ),
            Self::ImportFailed => f.write_str(
                "a stream frame failed to import into Vulkan or Skia, so it is released without \
                 being drawn and the stream draws nothing until the next frame; free GPU memory \
                 or let the producer draw smaller buffers when the cause is a lack of memory",
            ),
        }
    }
}

impl Painter {
    pub fn reject_stream_frame(&self, rejection: StreamRejection) {
        self.failures.record(DrawFailure::StreamRejected(rejection));
    }

    /// Records that a stream frame failed to import because of `cause`. The log line names `cause` and every source
    /// error below it.
    pub fn reject_stream_import(&self, cause: &(dyn std::error::Error + 'static)) {
        let failure = DrawFailure::StreamRejected(StreamRejection::ImportFailed);
        self.failures.record_caused(failure, cause);
    }
}
