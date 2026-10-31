// SPDX-License-Identifier: GPL-3.0-only
//! The error type returned by the tests of the renderer. It converts each error type that a test returns early with
//! the `?` operator.

use crate::failure::FailedDraw;
use crate::{DrawFailure, Error, SkslDiagnostic, StreamRejection};
use lavashell_core::prelude::NumberError;
use lavashell_core::scene::tree::ApplyError;
use lavashell_core::shape::PointsError;
use lavashell_test_fixtures::FrameFailure;
use log::SetLoggerError;
use std::array::TryFromSliceError;
use std::num::TryFromIntError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error(transparent)]
    Renderer(#[from] Error),
    #[error(transparent)]
    Draw(#[from] FailedDraw),
    #[error(transparent)]
    DrawFailure(#[from] DrawFailure),
    #[error(transparent)]
    Sksl(#[from] SkslDiagnostic),
    #[error(transparent)]
    Scene(#[from] ApplyError),
    #[error(transparent)]
    Failed(#[from] FrameFailure),
    #[error(transparent)]
    Points(#[from] PointsError),
    #[error(transparent)]
    Vulkan(#[from] lavashell_vulkan::Error),
    #[error("the stream frame is rejected: {0}")]
    Rejected(StreamRejection),
    #[error(transparent)]
    Number(#[from] NumberError),
    #[error("a number does not fit its integer type")]
    Integer(#[from] TryFromIntError),
    #[error("a texel does not have the bytes of a float")]
    Texel(#[from] TryFromSliceError),
    #[error("Skia cannot allocate a raster surface")]
    RasterSurface,
    #[error("the scale {0} is no density")]
    NoDensity(f64),
    #[error("a union needs at least one member")]
    NoMembers,
    #[error("the result has no {0}")]
    Missing(&'static str),
    #[error("the result is not {0}")]
    Unexpected(&'static str),
    #[error("LAVASHELL_TEST_FONT names no font file")]
    MissingTestFont,
    #[error("the test process has a logger that is not the recorder of the test")]
    Logger(#[source] &'static SetLoggerError),
}
