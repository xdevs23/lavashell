// SPDX-License-Identifier: GPL-3.0-only
use crate::platform::ndk::media_error::MediaError;
use crate::platform::{PropertiesError, ReleaseError};
use crate::surface_stream::BufferSizes;
use lavashell_core::error::message_chain;
use lavashell_renderer::{
    DrawError, Error as RendererError, FenceCopyError, FrameError, FrameOutcome, MultipleFailures,
    StartError, StateError,
};
use std::convert::Infallible;

/// A failure in the AOSP host integration. Each variant is named after the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot start the renderer")]
    Start(#[source] StateError),
    #[error("cannot attach the native window")]
    Attach(#[source] StateError),
    #[error("cannot request the output of the native window")]
    RequestOutput(#[source] StateError),
    #[error(
        "the native window reports {width} × {height} pixels, which is a failure status of AOSP"
    )]
    WindowSize { width: i32, height: i32 },
    #[error("the viewport covers {viewport:?} pixels, but the native window has {window:?} pixels")]
    ViewportSize {
        viewport: (u32, u32),
        window: (u32, u32),
    },
    #[error("the renderer state rejected the frame")]
    State(#[source] StateError),
    #[error("cannot draw the frame")]
    Draw(#[source] RendererError),
    #[error("cannot paint the frame")]
    Paint(#[source] RendererError),
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
    #[error("cannot read how the Vulkan device samples the buffer of a frame of a stream")]
    BufferProperties(#[source] PropertiesError),
    #[cfg(target_os = "android")]
    #[error("cannot read the format that the Vulkan device reports for a hardware buffer")]
    BufferFormat(#[source] PropertiesError),
    #[error("cannot import the buffer of a frame of a stream")]
    ImportBuffer(#[source] RendererError),
    #[error("cannot wrap the import of a frame of a stream for Skia")]
    WrapImport(#[source] RendererError),
    #[error("cannot hand the frames of streams back to their producers")]
    HandBack(#[source] ReleaseError),
    #[error(transparent)]
    CopyReleaseFence(#[from] FenceCopyError),
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
        let mut failures = self.into_failures();
        failures.extend(next.into_failures());
        Self::several(failures)
    }
}

impl From<StartError<Error>> for Error {
    fn from(failure: StartError<Self>) -> Self {
        match failure {
            StartError::State(cause) => Self::Start(cause),
            StartError::Streams(cause) => cause,
        }
    }
}

impl From<FrameError<Infallible>> for Error {
    fn from(failure: FrameError<Infallible>) -> Self {
        match failure {
            FrameError::State(cause) => Self::State(cause),
            FrameError::Draw(cause) => Self::Draw(cause),
            FrameError::Paint(cause) => Self::Paint(cause),
            FrameError::Control(never) => match never {},
        }
    }
}

/// A failure of the frame comes first, followed by the failures of the streams drawn by the frame.
impl From<DrawError<Infallible, Error>> for Error {
    fn from(failure: DrawError<Infallible, Self>) -> Self {
        match failure {
            DrawError::Frame(failure) => failure.into(),
            DrawError::StreamsOfShownFrame { drawn, streams } => Self::StreamsOfDrawnFrame {
                outcome: drawn.outcome,
                streams: Box::new(streams),
            },
            DrawError::FrameAndStreams { frame, streams } => Self::from(frame).then(streams),
        }
    }
}

impl MultipleFailures for Error {
    fn several(failures: Vec<Self>) -> Self {
        Self::Several(failures)
    }

    fn into_failures(self) -> Vec<Self> {
        match self {
            Self::Several(failures) => failures,
            failure => vec![failure],
        }
    }
}

/// The messages of the failures and of their causes, joined in the order in which the failures happened.
fn in_order(failures: &[Error]) -> String {
    let chains: Vec<_> = failures.iter().map(message_chain).collect();
    chains.join("; after that, ")
}
