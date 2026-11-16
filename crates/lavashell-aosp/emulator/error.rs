// SPDX-License-Identifier: GPL-3.0-only
use lavashell_aosp::{Aosp, AospError, BufferSizesError, PropertiesError, RunEnded};
use lavashell_renderer::{StreamRejection, skia};
use lavashell_runtime::{RunError, SetupError};
use ndk::media_error::MediaError;
use std::cell::BorrowMutError;
use std::num::TryFromIntError;
use std::sync::mpsc::RecvError;

#[derive(Debug, thiserror::Error)]
pub enum NativeMethodError {
    #[error("a length of the buffers of the stream is negative")]
    NegativeLength(#[source] TryFromIntError),
    #[error("the buffer sizes of the stream are not valid")]
    BufferSizes(#[source] BufferSizesError),
    #[error("the id {0} does not match a stream format")]
    UnknownFormat(i32),
    #[error("cannot set up the stream that the producer draws into")]
    SetUp(#[source] lavashell_aosp::Error),
    #[error("cannot create the target ImageReader, the listener of its frames or its window")]
    CreateTarget(#[source] MediaError),
    #[error("cannot start the thread of the run that draws into the target ImageReader")]
    SpawnRun(#[source] std::io::Error),
    #[error("cannot add the window of the pane to the app")]
    AddWindow(#[source] SetupError),
    #[error("the host cannot execute the run that draws into the target ImageReader")]
    Serve(#[source] AospError),
    #[error("the run drawing into the target ImageReader failed")]
    Run(#[source] RunError<Aosp>),
    #[error("the run drawing into the target ImageReader ended before the round trip finished")]
    RunEndedEarly,
    #[error("the renderer used in the round trip has stopped")]
    RunEnded(#[source] RunEnded),
    #[error("the stream used in the round trip is not attached to the renderer")]
    FindStream(#[source] lavashell_aosp::Error),
    #[error("cannot resize the stream")]
    Resize(#[source] lavashell_aosp::Error),
    #[error("cannot hand the Surface of the stream to the activity")]
    Surface(#[source] lavashell_aosp::Error),
    #[error("cannot latch a frame of the stream")]
    Latch(#[source] lavashell_aosp::Error),
    #[error("cannot copy the fence of the frame that the producer has not finished writing")]
    CopyProducerFence(#[source] std::io::Error),
    #[error("the stream released its newest frame without drawing it: {0}")]
    FrameRejected(StreamRejection),
    #[error("cannot wait until the producer has written the frame waiting in the stream")]
    WaitForProducer(#[source] rustix::io::Errno),
    #[error("cannot wait for a frame notification from the stream or from the target ImageReader")]
    AwaitNotification(#[source] RecvError),
    #[error("no frame has been latched from the stream")]
    NothingLatched,
    #[error("the target ImageReader announced a frame but has none")]
    NoTargetFrame,
    #[error("cannot read the frame presented to the target ImageReader")]
    ReadTarget(#[source] MediaError),
    #[error("the pixel at {0:?} lies outside the frame presented to the target ImageReader")]
    PixelOutside(skia::IPoint),
    #[error("cannot read the format that Vulkan reports for the buffer")]
    BufferFormat(#[source] PropertiesError),
    #[error("the Vulkan driver cannot import the buffer read for its format")]
    UnimportableBuffer,
    #[error("the YCbCr conversion does not know the model or the range of the buffer")]
    YcbcrConversion(#[source] crate::ycbcr::Error),
    #[error("the YCbCr codes of the color or their tolerance lie outside 8 bits")]
    EncodeColor,
    #[error("cannot check whether the hardware buffer to read is closed")]
    InspectBuffer(#[source] jni::errors::Error),
    #[error("the hardware buffer to read is closed")]
    ClosedBuffer,
    #[error("cannot read the MotionEvent to report")]
    ReadMotionEvent(#[source] lavashell_aosp::Error),
    #[error("the handle of a new round trip exceeds the range of a 64-bit JNI long")]
    TooManyRoundTrips(#[source] TryFromIntError),
    #[error("cannot create the object or array returned to the activity")]
    ReturnToActivity(#[source] jni::errors::Error),
    #[error("the round trips of this thread are in use by another call")]
    RoundTripsInUse(#[source] BorrowMutError),
    #[error("no round trip with the handle {0} exists on this thread")]
    UnknownHandle(i64),
}
