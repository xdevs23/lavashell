// SPDX-License-Identifier: GPL-3.0-only
use crate::platform::ndk::media_error::MediaError;
use crate::platform::{PropertiesError, ReleaseError};
use crate::surface_stream::BufferSizes;
use lavashell_core::error::message_chain;
use lavashell_core::prelude::StreamId;
use lavashell_renderer::{Error as RendererError, FenceCopyError, MultipleFailures};

/// A failure in the AOSP host integration. Each variant is named after the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "the native window reports {width} × {height} pixels, which is a failure status of AOSP"
    )]
    WindowSize { width: i32, height: i32 },
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
    #[error("no stream with the id {0:?} is attached to the renderer")]
    FindStream(StreamId),
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
