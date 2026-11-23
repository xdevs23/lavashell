// SPDX-License-Identifier: GPL-3.0-only

use crate::Error;
use crate::paint::StreamRejection;
use lavashell_error_chain::message_chain;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
use std::{cell::RefCell, collections::BTreeMap, fmt, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DrawFailure {
    Allocation,
    SurfaceRead,
    LensWithoutSurface,
    UnbalancedRestore,
    ImageSize,
    ImageUndecodable,
    ImagePixels,
    StreamUnbound,
    StreamStale,
    StreamRejected(StreamRejection),
    ShaderRejected,
    GpuTimestamp,
    ColorSpace,
    SingularTransform,
    OutlineLength,
    TileSize,
    SubpixelText,
    NumberRange,
}

impl NamesNextStep for DrawFailure {
    fn next_step(&self) -> NextStep {
        match self {
            Self::Allocation | Self::StreamRejected(StreamRejection::ImportFailed) => {
                NextStep::FreeGpuMemory
            }
            Self::StreamRejected(
                StreamRejection::TooLarge
                | StreamRejection::UnusableBuffer
                | StreamRejection::BufferFormat(_)
                | StreamRejection::CropOutside,
            ) => NextStep::ChangeTheStreamProducer,
            Self::GpuTimestamp => NextStep::UpdateGraphicsDriver,
            Self::SubpixelText => NextStep::FreeSystemResources,
            Self::ColorSpace | Self::TileSize | Self::ShaderRejected => NextStep::ReportTheFailure,
            Self::SurfaceRead
            | Self::LensWithoutSurface
            | Self::UnbalancedRestore
            | Self::ImageSize
            | Self::ImageUndecodable
            | Self::ImagePixels
            | Self::StreamUnbound
            | Self::StreamStale
            | Self::SingularTransform
            | Self::OutlineLength
            | Self::NumberRange => NextStep::ChangeTheScene,
        }
    }
}

impl fmt::Display for DrawFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Allocation => {
                "cannot allocate an offscreen texture, so the renderer leaves out shapes that need one"
            }
            Self::SurfaceRead => {
                "cannot read the surface below glass, so the renderer leaves out the glass"
            }
            Self::LensWithoutSurface => {
                "glass on a union or a long contour cannot read the surface inside an \
                 opacity group, a content filter or encoded-sRGB blending, or under rotation \
                 or skew, so it blurs and tints without its lens"
            }
            Self::UnbalancedRestore => {
                "a command list restores more often than it saves, so the renderer leaves out the \
                 leaf or ignores the extra restore"
            }
            Self::ImageSize => {
                "an image declares another size than its decoded pixels, so the renderer leaves out \
                 the image"
            }
            Self::ImageUndecodable => {
                "an image has encoded bytes that Skia cannot decode, so the renderer leaves out the \
                 image"
            }
            Self::ImagePixels => {
                "an image has more pixels than the largest image that the GPU supports, so the \
                 renderer does not decode it and leaves it out"
            }
            Self::StreamUnbound => {
                "the program paints a stream image without a frame bound for the stream that the \
                 image shows, so the renderer leaves out the image"
            }
            Self::StreamStale => {
                "a stream image belongs to another frame than the frame bound for the stream, so \
                 the renderer leaves out the image"
            }
            Self::StreamRejected(rejection) => return fmt::Display::fmt(&rejection, f),
            Self::GpuTimestamp => {
                "cannot write or read a GPU timestamp, so the renderer does not measure the GPU time \
                 spent on a frame or an effect"
            }
            Self::ColorSpace => {
                "Skia cannot build the working color space, so the renderer paints retained layers \
                 again on every frame instead of caching them"
            }
            Self::SingularTransform => {
                "a transform that scales glass to almost nothing cannot be inverted, so the renderer \
                 leaves out the glass"
            }
            Self::OutlineLength => {
                "a contour has more segments than a shader float counts exactly, so the renderer \
                 leaves out the contour"
            }
            Self::TileSize => {
                "a raster tile is empty or has more bytes than the GPU context's resource cache \
                 allows, so the renderer leaves out the shape that needs it"
            }
            Self::SubpixelText => {
                "subpixel text cannot be rasterized as subpixel coverage, so the renderer draws it \
                 with the ordinary text rendering"
            }
            Self::NumberRange => {
                "a length, a color or another number of a draw lies beyond the range of the \
                 floats that Skia and the GPU use, so the renderer leaves out the draw"
            }
            Self::ShaderRejected => {
                "Skia rejects a shader that the renderer builds, so the renderer leaves out the \
                 draw that needs it, or draws it without the shader"
            }
        };
        f.write_str(message)
    }
}

impl std::error::Error for DrawFailure {}

#[derive(Debug)]
pub struct FailedDraw {
    failure: DrawFailure,
    cause: Option<Rc<Error>>,
}

impl FailedDraw {
    pub(crate) fn caused(failure: DrawFailure, cause: Rc<Error>) -> Self {
        Self {
            failure,
            cause: Some(cause),
        }
    }

    #[must_use]
    pub fn failure(&self) -> DrawFailure {
        self.failure
    }
}

impl fmt::Display for FailedDraw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.failure, f)
    }
}

impl std::error::Error for FailedDraw {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause
            .as_deref()
            .map(|cause| cause as &(dyn std::error::Error + 'static))
    }
}

impl From<DrawFailure> for FailedDraw {
    fn from(failure: DrawFailure) -> Self {
        Self {
            failure,
            cause: None,
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct Failures(Rc<RefCell<BTreeMap<DrawFailure, u64>>>);

impl Failures {
    pub fn record(&self, failed: impl Into<FailedDraw>) {
        let FailedDraw { failure, cause } = failed.into();
        self.log(
            failure,
            cause
                .as_deref()
                .map(|cause| cause as &(dyn std::error::Error + 'static)),
        );
    }

    pub fn record_caused(&self, failure: DrawFailure, cause: &(dyn std::error::Error + 'static)) {
        self.log(failure, Some(cause));
    }

    fn log(&self, failure: DrawFailure, cause: Option<&(dyn std::error::Error + 'static)>) {
        let Some(count) = self.count_logged(failure) else {
            return;
        };
        let next_step = (cause.and_then(<dyn std::error::Error>::downcast_ref::<Error>))
            .map_or(failure.next_step(), Error::next_step);
        let cause = fmt::from_fn(|f| match cause {
            Some(cause) => write!(f, "; the cause this time: {}", message_chain(cause)),
            None => Ok(()),
        });
        log::warn!(
            "{failure}{cause} (recorded {count} times since the renderer started; the renderer logs \
             it each time the count doubles). {next_step}"
        );
    }

    fn count_logged(&self, failure: DrawFailure) -> Option<u64> {
        let mut counts = self.0.borrow_mut();
        let count = counts.entry(failure).or_default();
        *count += 1;
        count.is_power_of_two().then_some(*count)
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Failures {
    pub(crate) fn count(&self, failure: DrawFailure) -> u64 {
        self.0.borrow().get(&failure).copied().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;
