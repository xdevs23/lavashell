// SPDX-License-Identifier: GPL-3.0-only
use crate::platform::ReleaseError;
use crate::platform::ndk::media_error::MediaError;
use crate::surface_stream::BufferSizes;
use lavashell_core::error::message_chain;
use lavashell_renderer::{Error as RendererError, FrameOutcome};

/// A failure in the AOSP host integration. Each variant is named after the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot create the Vulkan renderer")]
    CreateRenderer(#[source] RendererError),
    #[error("cannot create the painter of the renderer")]
    CreatePainter(#[source] RendererError),
    #[error("cannot attach the native window")]
    Attach(#[source] RendererError),
    #[error("no native window is attached")]
    Detached,
    #[error(
        "the native window reports {width} × {height} pixels, which is a failure status of AOSP"
    )]
    WindowSize { width: i32, height: i32 },
    #[error("cannot draw the frame")]
    Draw(#[source] RendererError),
    #[error(
        "the frame was drawn with the outcome {outcome:?}, but the frames of streams it drew failed"
    )]
    StreamsOfDrawnFrame {
        outcome: FrameOutcome,
        #[source]
        streams: Box<Error>,
    },
    #[error("the buffer sizes {0:?} exceed the signed 32-bit lengths of the NDK")]
    SizesBeyondNdk(BufferSizes),
    #[error(
        "the default size of the buffers in {0:?} exceeds the maximum, so every frame drawn at the default size \
         would be released without being drawn"
    )]
    DefaultBeyondMax(BufferSizes),
    #[error("cannot create the image reader of a stream")]
    CreateReader(#[source] MediaError),
    #[error("cannot listen for the buffers that the producer of a stream frees")]
    ListenForRemovals(#[source] MediaError),
    #[error("cannot listen for the frames of a stream")]
    ListenForFrames(#[source] MediaError),
    #[error("cannot get the native window of the image reader of a stream")]
    ReaderWindow(#[source] MediaError),
    #[error("cannot acquire a frame of a stream")]
    AcquireFrame(#[source] MediaError),
    #[error("cannot read the hardware buffer of a frame of a stream")]
    FrameBuffer(#[source] MediaError),
    #[error("cannot read the id of the hardware buffer of a frame of a stream")]
    BufferId(#[source] std::io::Error),
    #[error("cannot read the id of a buffer that the producer of a stream freed")]
    RemovedBufferId(#[source] std::io::Error),
    #[error("cannot read the crop of a frame of a stream")]
    FrameCrop(#[source] MediaError),
    #[error("a length of the crop of a frame of a stream is negative")]
    CropLength(#[source] std::num::TryFromIntError),
    #[error("cannot check whether the producer of a stream has written a frame")]
    PollAcquireFence(#[source] rustix::io::Errno),
    #[error("the Vulkan device cannot import the frames of streams")]
    Importer(#[source] RendererError),
    #[error("a frame of the stream is still latched after the frame latched before was returned")]
    StillLatched,
    #[error("cannot read how the Vulkan device samples the buffer of a frame of a stream")]
    BufferProperties(#[source] RendererError),
    #[cfg(target_os = "android")]
    #[error("cannot read the format that the Vulkan device reports for a hardware buffer")]
    BufferFormat(#[source] RendererError),
    #[error("cannot import the buffer of a frame of a stream")]
    ImportBuffer(#[source] RendererError),
    #[error("cannot wrap the import of a frame of a stream for Skia")]
    WrapImport(#[source] RendererError),
    #[error("cannot hand the frames of streams back to their producers")]
    HandBack(#[source] ReleaseError),
    #[error("cannot copy the release fence of a frame of a stream")]
    CopyReleaseFence(#[source] std::io::Error),
    #[error("cannot submit the completion of the GPU work that reads the frames of streams")]
    SubmitCompletion(#[source] RendererError),
    #[error("cannot read the completion of the GPU work that reads the frames of streams")]
    CompletionStatus(#[source] RendererError),
    #[error("cannot destroy the imports of frames that the GPU has finished with")]
    RetireImports(#[source] RendererError),
    #[cfg(target_os = "android")]
    #[error(
        "a Java exception is pending, so JNI cannot convert between a Surface and its native window"
    )]
    PendingException,
    #[cfg(target_os = "android")]
    #[error("the Surface to convert is null")]
    NullSurface,
    #[cfg(target_os = "android")]
    #[error("cannot check whether the object to convert is an android.view.Surface")]
    SurfaceClass(#[source] jni::errors::Error),
    #[cfg(target_os = "android")]
    #[error("the object to convert is no android.view.Surface")]
    NotASurface,
    #[cfg(target_os = "android")]
    #[error("AOSP returned no native window for the Surface")]
    SurfaceWindow,
    #[cfg(target_os = "android")]
    #[error("AOSP returned no Surface for the native window")]
    WindowSurface,
    /// Failures of one call, in the order in which they happened. After each failure, the call continued with the work
    /// that did not depend on the failed work.
    #[error("{}", in_order(.0))]
    Several(Vec<Error>),
}

impl Error {
    /// Combines this failure and `next`, a later failure in the same call, into `Several` in that order.
    #[must_use]
    pub(crate) fn then(self, next: Self) -> Self {
        let mut failures = Failures::default();
        failures.push(self);
        failures.push(next);
        Self::Several(failures.0)
    }
}

/// The messages of the failures and of their causes, joined in the order in which the failures happened.
fn in_order(failures: &[Error]) -> String {
    let chains: Vec<_> = failures.iter().map(message_chain).collect();
    chains.join("; after that, ")
}

/// Collects the failures of a call that continues after a failure.
#[derive(Default)]
pub(crate) struct Failures(Vec<Error>);

impl Failures {
    pub fn add(&mut self, result: Result<(), Error>) {
        if let Err(error) = result {
            self.push(error);
        }
    }

    pub fn push(&mut self, error: Error) {
        match error {
            Error::Several(errors) => self.0.extend(errors),
            error => self.0.push(error),
        }
    }

    pub fn into_result(mut self) -> Result<(), Error> {
        if self.0.len() > 1 {
            return Err(Error::Several(self.0));
        }
        self.0.pop().map_or(Ok(()), Err)
    }
}
