// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::host::AospError;
use lavashell_actor::Closed;
use lavashell_core::scene::tree::ApplyError;
use lavashell_core::units::NumberError;
use lavashell_renderer::{Error as RendererError, Latch};
use log::SetLoggerError;

/// A failure of a test or of the platform that the tests simulate.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error(transparent)]
    Adapter(#[from] Error),
    #[error("the simulated renderer failed")]
    Renderer(#[from] RendererError),
    #[error("the platform of the AOSP host failed")]
    Aosp(#[from] AospError),
    #[error("the mailbox of the reports closed")]
    Closed(#[from] Closed),
    #[error("cannot build the scene of a test")]
    Scene(#[from] ApplyError),
    #[error("a number of a test is not a valid value")]
    Number(#[from] NumberError),
    #[error("cannot install the logger that records the warnings of a test")]
    InstallLogger(#[source] &'static SetLoggerError),
    #[error("the latch found no new frame, but {0:?}")]
    NoNewFrame(Latch),
    #[error("the pixel {x}, {y} lies outside the simulated surface")]
    PixelOutside { x: i32, y: i32 },
    #[error("the native window belongs to a display or to a deleted reader")]
    NotAReaderWindow,
    #[error("the producer is not writing the buffer {0}")]
    NotBeingWritten(u64),
    #[error("cannot signal the fence of a buffer being written")]
    SignalFence(#[source] std::io::Error),
    #[error("cannot read which file a fence refers to")]
    FenceInode(#[source] std::io::Error),
}
