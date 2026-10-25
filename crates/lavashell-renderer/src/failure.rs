// SPDX-License-Identifier: GPL-3.0-only
//! Failures of drawing a frame, counted per cause and reported through the `log` facade.

use crate::Error;
use crate::paint::StreamRejection;
use lavashell_core::error::message_chain;
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

impl fmt::Display for DrawFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Allocation => {
                "cannot allocate an offscreen texture, so shapes that need one are left out; \
                 free GPU memory or lower the window size"
            }
            Self::SurfaceRead => {
                "cannot read the surface below glass, so the glass is left out; \
                 paint glass onto a surface"
            }
            Self::LensWithoutSurface => {
                "glass on a union or a long contour cannot read the surface inside an \
                 opacity group, a content filter or encoded-sRGB blending, or under rotation \
                 or skew, so it blurs and tints without its lens; paint that glass straight \
                 onto the surface, unrotated"
            }
            Self::UnbalancedRestore => {
                "a command list restores more often than it saves, so the extra restore is \
                 ignored, as every other reader of the list ignores it; pair each restore \
                 with a save, an opacity group or an effect layer"
            }
            Self::ImageSize => {
                "an image declares another size than its decoded pixels, so it is left out; \
                 give Image::encoded the width and height of the decoded image"
            }
            Self::ImageUndecodable => {
                "an image has encoded bytes that Skia cannot decode, so it is left out; give \
                 Image::encoded the bytes of an image in a format that Skia decodes"
            }
            Self::ImagePixels => {
                "an image has more pixels than the largest image that the GPU holds, so it is not \
                 decoded and is left out; give Image::encoded a smaller image"
            }
            Self::StreamUnbound => {
                "a stream image is painted without a frame bound for the stream of the image, so \
                 the image is left out; bind the frame of every stream drawn by the commands \
                 before painting them"
            }
            Self::StreamStale => {
                "a stream image belongs to another frame than the frame bound for the stream, so \
                 it is left out; draw the stream image of the frame that was bound last"
            }
            Self::StreamRejected(rejection) => return rejection.fmt(f),
            Self::GpuTimestamp => {
                "cannot write or read a GPU timestamp, so the GPU time of a frame or an effect is \
                 not measured; update the graphics driver, or report the failure with this log"
            }
            Self::ColorSpace => {
                "Skia cannot build the working color space, so retained layers are painted again \
                 on every frame instead of being cached; this is a defect in this build of the \
                 renderer, so report it with this log"
            }
            Self::SingularTransform => {
                "glass is drawn under a transform that scales it to almost nothing, so the \
                 transform cannot be inverted and the glass is left out; draw the glass at a \
                 visible scale"
            }
            Self::OutlineLength => {
                "a contour has more segments than a shader float counts exactly, so it is left out; \
                 draw it as several shorter contours"
            }
            Self::TileSize => {
                "a raster tile has no pixels or more bytes than the resource cache of the GPU \
                 context holds, so the shape that needs it is left out; this is a defect in this \
                 build of the renderer, so report it with this log"
            }
            Self::SubpixelText => {
                "subpixel text cannot be rasterized as subpixel coverage, so it is drawn with the \
                 ordinary text rendering; free memory, or report the failure with this log"
            }
            Self::NumberRange => {
                "a length, a color or another number of a draw lies beyond the range of the \
                 floats that Skia and the GPU take, so the draw is left out; keep the numbers of \
                 the scene within that range"
            }
            Self::ShaderRejected => {
                "Skia rejects a shader that the renderer builds, so the draw that needs it is left \
                 out or drawn without it; this is a defect in this build of the renderer, so \
                 report it with this log"
            }
        };
        f.write_str(message)
    }
}

impl std::error::Error for DrawFailure {}

/// A draw that failed: the kind of failure that prevented it, and the renderer error that caused it, if there is one.
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

#[derive(Default)]
pub(crate) struct Failures(RefCell<BTreeMap<DrawFailure, u64>>);

impl Failures {
    /// Counts the failure in `failed` and logs it, with its cause if it has one. A failure is logged as a warning
    /// the first time and whenever its count doubles, so that a failure repeated in every frame is still logged
    /// without filling the log.
    pub fn record(&self, failed: impl Into<FailedDraw>) {
        let FailedDraw { failure, cause } = failed.into();
        if let Some(cause) = cause {
            self.record_caused(failure, &*cause);
        } else if let Some(count) = self.count_logged(failure) {
            log::warn!(
                "{failure} (recorded {count} times since the renderer started; logged each time the \
                 count doubles)"
            );
        }
    }

    /// Counts `failure` as `record` does, and logs `cause` with its sources along with it.
    pub fn record_caused(&self, failure: DrawFailure, cause: &(dyn std::error::Error + 'static)) {
        if let Some(count) = self.count_logged(failure) {
            let cause = message_chain(cause);
            log::warn!(
                "{failure}; the cause this time: {cause} (recorded {count} times since the renderer \
                 started; logged each time the count doubles)"
            );
        }
    }

    /// Counts `failure` and returns the new count when it is a power of two, which is when the failure is logged.
    fn count_logged(&self, failure: DrawFailure) -> Option<u64> {
        let mut counts = self.0.borrow_mut();
        let count = counts.entry(failure).or_default();
        *count += 1;
        count.is_power_of_two().then_some(*count)
    }
}

#[cfg(test)]
mod tests;
