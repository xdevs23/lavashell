// SPDX-License-Identifier: GPL-3.0-only
#[cfg(test)]
mod tests;

use super::Painter;
use super::image::draw_framed;
use crate::failure::DrawFailure;
use lavashell_core::prelude::*;
use skia_safe::{Canvas, IPoint, IRect};
use std::collections::BTreeMap;
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

pub trait StreamImportHandle {}

pub struct StreamPixels {
    image: skia_safe::Image,
    import: Option<Rc<dyn StreamImportHandle>>,
}

impl StreamPixels {
    #[must_use]
    pub fn imported(image: skia_safe::Image, import: Rc<dyn StreamImportHandle>) -> Self {
        Self {
            image,
            import: Some(import),
        }
    }

    fn release(self) {
        let Self { image, import } = self;
        drop(image);
        drop(import);
    }
}

impl From<skia_safe::Image> for StreamPixels {
    fn from(image: skia_safe::Image) -> Self {
        Self {
            image,
            import: None,
        }
    }
}

impl StreamFrames {
    pub fn bind(&mut self, image: &StreamImage, pixels: StreamPixels, origin: IPoint) {
        let frame = BoundFrame {
            generation: image.generation,
            pixels,
            origin,
        };
        if let Some(replaced) = self.bound.insert(image.stream, frame) {
            replaced.pixels.release();
        }
    }

    pub fn retire(&mut self, stream: StreamId) {
        if let Some(retired) = self.bound.remove(&stream) {
            retired.pixels.release();
        }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnsampledFormat {
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

    pub fn reject_stream_import(&self, cause: &(dyn std::error::Error + 'static)) {
        let failure = DrawFailure::StreamRejected(StreamRejection::ImportFailed);
        self.failures.record_caused(failure, cause);
    }
}
