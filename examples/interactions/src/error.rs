// SPDX-License-Identifier: GPL-3.0-only
use lavashell::core::transfer::TransferError;
use lavashell::error_chain::next_step::NextStep;
use lavashell::prelude::NumberError;
use lavashell_example_support::{ExampleError, ExampleStep};
use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Example(#[from] lavashell_example_support::Error),
    #[error(transparent)]
    Setup(#[from] lavashell::SetupError),
    #[error("no home directory to keep the sample files in")]
    NoHome,
    #[error("cannot create the cache directory {path}")]
    CreateCache {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot create the directory of the sample files")]
    CreateSamples(#[source] io::Error),
    #[error("cannot write the sample file {path}")]
    WriteSample {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot resolve the sample file {path}")]
    ResolveSample {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the sample file {0} has no file URI")]
    FileUri(PathBuf),
    #[error("cannot offer the sample")]
    Offer(#[source] TransferError),
    #[error("the cut text changed before the move ended and stays")]
    CutTextChanged,
    #[error("cannot read the size of the image")]
    ReadImage(#[source] lavashell::renderer::Error),
    #[error("cannot find the executable of the example")]
    CurrentExecutable(#[source] io::Error),
    #[error("cannot start the shaped window")]
    StartChild(#[source] io::Error),
    #[error("cannot watch the process of the shaped window")]
    WatchChild(#[source] rustix::io::Errno),
    #[error("cannot read whether the process of the shaped window ended")]
    ChildStatus(#[source] io::Error),
    #[error("cannot end the process of the shaped window")]
    EndChild(#[source] io::Error),
    #[error("cannot wait for the end of the process of the shaped window")]
    ReapChild(#[source] io::Error),
    #[error("cannot count the corners of the round hole of the shaped window")]
    HoleCorners(#[source] NumberError),
}

impl ExampleError for Error {
    fn next_step(&self) -> ExampleStep {
        match self {
            Self::Example(error) => error.next_step(),
            Self::NoHome => ExampleStep::SetHome,
            Self::CreateCache { .. } | Self::CreateSamples(_) | Self::WriteSample { .. } => {
                ExampleStep::MakeCacheWritable
            }
            Self::Setup(_)
            | Self::ResolveSample { .. }
            | Self::FileUri(_)
            | Self::Offer(_)
            | Self::CutTextChanged
            | Self::ReadImage(_)
            | Self::CurrentExecutable(_)
            | Self::StartChild(_)
            | Self::WatchChild(_)
            | Self::ChildStatus(_)
            | Self::EndChild(_)
            | Self::ReapChild(_)
            | Self::HoleCorners(_) => NextStep::ReportTheFailure.into(),
        }
    }
}
