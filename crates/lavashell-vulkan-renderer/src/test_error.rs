// SPDX-License-Identifier: GPL-3.0-only

use crate::{CommitError, DrawError, Error, StartError, StateError};
use lavashell_core::prelude::NumberError;
use lavashell_core::retained::UnbalancedRestore;
use lavashell_core::scene::tree::ApplyError;
use lavashell_renderer::{DrawFailure, StreamRejection};
use std::convert::Infallible;
use std::num::TryFromIntError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error(transparent)]
    VulkanRenderer(#[from] Error),
    #[error(transparent)]
    Renderer(#[from] lavashell_renderer::Error),
    #[error(transparent)]
    RendererTest(#[from] lavashell_renderer::test_error::TestError),
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    StateDraw(#[from] DrawError<Infallible>),
    #[error(transparent)]
    StartWithStreams(#[from] StartError<Error>),
    #[error(transparent)]
    StateCommit(#[from] CommitError<Infallible>),
    #[error(transparent)]
    DrawFailure(#[from] DrawFailure),
    #[error(transparent)]
    Scene(#[from] ApplyError),
    #[error(transparent)]
    Packet(#[from] UnbalancedRestore),
    #[error(transparent)]
    Vulkan(#[from] lavashell_vulkan::Error),
    #[error("the stream frame is rejected: {0}")]
    Rejected(StreamRejection),
    #[error(transparent)]
    Number(#[from] NumberError),
    #[error("a number does not fit in the integer type")]
    Integer(#[from] TryFromIntError),
    #[error("Skia cannot allocate a raster surface")]
    RasterSurface,
    #[error("the result does not contain {0}")]
    Missing(&'static str),
    #[error("the result is not {0}")]
    Unexpected(&'static str),
}
